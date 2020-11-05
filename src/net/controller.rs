use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::time::{Instant, Duration};
use std::thread;

use bincode;
use astro_dnssd::browser::{ServiceBrowserBuilder, ServiceEventType};
use tokio::{task::JoinHandle, runtime::Runtime};

use crate::*;

/// Controller used for discovering available Karl services via DNS-SD.
///
/// Currently, each client runs its own controller, which is aware of all
/// available Karl services. Eventually, there may be a central controller
/// that coordinates client requests among available services.
/// Non-macOS services need to install the appropriate shims around DNS-SD.
pub struct Controller {
    pub rt: Runtime,
    blocking: bool,
    hosts: Arc<Mutex<Vec<SocketAddr>>>,
    prev_host_i: usize,
}

impl Controller {
    /// Create a new controller.
    ///
    /// The controller spawns a process in the background that listens on
    /// DNS-SD for available hosts. The controller maintains a list of
    /// available hosts, adding and removing hosts as specified by DNS-SD
    /// messages. On request, a host selected by the controller is not
    /// guaranteed to be available, and the client may have to try again.
    ///
    /// Parameters:
    /// - `rt`: The Tokio runtime.
    /// - `blocking`: Whether the controller should block until it finds
    ///   an available host on request. Otherwise, if no hosts are available,
    ///   the controller will error.
    pub fn new(rt: Runtime, blocking: bool) -> Self {
        let c = Controller {
            rt,
            blocking,
            hosts: Arc::new(Mutex::new(Vec::new())),
            prev_host_i: 0,
        };
        let hosts = c.hosts.clone();
        debug!("Listening...");
        c.rt.spawn(async move {
            let mut browser = ServiceBrowserBuilder::new("_karl._tcp")
                .build()
                .unwrap();
            browser.start(move |result| match result {
                Ok(mut service) => {
                    let results = service.resolve();
                    for r in results.unwrap() {
                        let status = r.txt_record.as_ref().unwrap().get("status");
                        let addrs_iter = match r.to_socket_addrs() {
                            Ok(addrs) => addrs,
                            Err(e) => {
                                error!("Failed to resolve\n=> addrs: {:?}\n=> {:?}", r, e);
                                return;
                            },
                        };
                        for addr in addrs_iter {
                            if !addr.is_ipv4() {
                                continue;
                            }
                            // Update hosts with IPv4 address.
                            // Log the discovered service
                            debug!("Addr: {}", addr);
                            match service.event_type {
                                ServiceEventType::Added => {
                                    info!(
                                        "ADD if: {} name: {} type: {} domain: {} => {:?}",
                                        service.interface_index, service.name,
                                        service.regtype, service.domain, addr,
                                    );
                                    hosts.lock().unwrap().push(addr);
                                },
                                ServiceEventType::Removed => {
                                    info!(
                                        "RMV if: {} name: {} type: {} domain: {} => {:?}",
                                        service.interface_index, service.name,
                                        service.regtype, service.domain, addr,
                                    );
                                    let mut hosts = hosts.lock().unwrap();
                                    for i in 0..hosts.len() {
                                        if hosts[i] == addr {
                                            hosts.remove(i);
                                            break;
                                        }
                                    }
                                },
                            }
                        }
                        debug!("Status: {:?}", status);
                    }
                }
                Err(e) => error!("Error: {:?}", e),
            }).unwrap();
            loop {
                if browser.has_data() {
                    browser.process_result();
                }
            }
        });
        c
    }

    /// Find a host to connect to round-robin.
    fn find_host(&mut self) -> Result<SocketAddr, Error> {
        let hosts = loop {
            let hosts = self.hosts.lock().unwrap();
            if !hosts.is_empty() {
                break hosts;
            }
            if !self.blocking {
                return Err(Error::NoAvailableHosts);
            }
            drop(hosts);
            trace!("No hosts found! Try again in 1 second...");
            thread::sleep(Duration::from_secs(1));
        };
        let i = (self.prev_host_i + 1) % hosts.len();
        self.prev_host_i = i;
        Ok(hosts[i])
    }

    /// Connect to a host, returning the tcp stream.
    fn connect(&mut self) -> Result<TcpStream, Error> {
        debug!("connect...");
        let now = Instant::now();
        let host = self.find_host()?;
        debug!("=> {} s ({:?})", now.elapsed().as_secs_f32(), host);
        let stream = TcpStream::connect(&host)?;
        debug!("=> {} s (connect)", now.elapsed().as_secs_f32());
        Ok(stream)
    }

    /// Send a request to the connected host.
    fn send(
        mut stream: TcpStream,
        req_bytes: Vec<u8>,
        req_ty: HeaderType,
    ) -> Result<(Header, Vec<u8>), Error> {
        let now = Instant::now();
        write_packet(&mut stream, req_ty, &req_bytes)?;
        debug!("=> {} s (write to stream)", now.elapsed().as_secs_f32());

        // Wait for the response.
        debug!("waiting for response...");
        let now = Instant::now();
        let mut bytes = read_packets(&mut stream, 1)?;
        debug!("=> {} s (read from stream)", now.elapsed().as_secs_f32());
        Ok(bytes.remove(0))
    }

    /// Ping the host.
    pub fn ping(&mut self) -> Result<PingResult, Error> {
        let stream = self.connect()?;
        let req = PingRequest::new();
        let now = Instant::now();
        info!("sending {:?} to {:?}...", req, stream.peer_addr());
        debug!("serializing request");
        let req_bytes = bincode::serialize(&req)
            .map_err(|e| Error::SerializationError(format!("{:?}", e)))?;
        let req_ty = HT_PING_REQUEST;
        debug!("=> {} s", now.elapsed().as_secs_f32());
        let (header, res_bytes) = Controller::send(stream, req_bytes, req_ty)?;
        let now = Instant::now();
        let res: PingResult = match header.ty {
            HT_PING_RESULT => bincode::deserialize(&res_bytes)
                .map_err(|e| Error::SerializationError(format!("{:?}", e)))?,
            ty => { return Err(Error::InvalidPacketType(ty)); },
        };
        debug!("=> {} s (deserialize)", now.elapsed().as_secs_f32());
        Ok(res)
    }

    /// Execute a compute request and return the result.
    ///
    /// Errors if there are network connection issues with the service.
    /// The controller may have found a service that is no longer available,
    /// or disconnected after the initial handshake. In this case, the
    /// client should try again.
    pub fn compute(
        &mut self,
        req: ComputeRequest,
    ) -> Result<ComputeResult, Error> {
        let stream = self.connect()?;
        let now = Instant::now();
        info!("sending {:?} to {:?}...", req, stream.peer_addr());
        debug!("serializing request");
        let req_bytes = bincode::serialize(&req)
            .map_err(|e| Error::SerializationError(format!("{:?}", e)))?;
        let req_ty = HT_COMPUTE_REQUEST;
        debug!("=> {} s", now.elapsed().as_secs_f32());
        let (header, res_bytes) = Controller::send(stream, req_bytes, req_ty)?;
        let now = Instant::now();
        let res: ComputeResult = match header.ty {
            HT_COMPUTE_RESULT => bincode::deserialize(&res_bytes)
                .map_err(|e| Error::SerializationError(format!("{:?}", e)))?,
            ty => { return Err(Error::InvalidPacketType(ty)); },
        };
        debug!("=> {} s (deserialize)", now.elapsed().as_secs_f32());
        Ok(res)
    }

    /// Asynchronously execute a compute request and return a handle that
    /// returns the result.
    pub fn compute_async(
        &mut self,
        req: ComputeRequest,
    ) -> Result<JoinHandle<Result<ComputeResult, Error>>, Error> {
        let stream = self.connect()?;
        let now = Instant::now();
        debug!("serializing request");
        let req_bytes = bincode::serialize(&req)
            .map_err(|e| Error::SerializationError(format!("{:?}", e)))?;
        let req_ty = HT_COMPUTE_REQUEST;
        debug!("=> {} s", now.elapsed().as_secs_f32());
        Ok(self.rt.spawn(async move {
            let (header, res_bytes) = Controller::send(stream, req_bytes, req_ty)?;
            let now = Instant::now();
            let res: ComputeResult = match header.ty {
                HT_COMPUTE_RESULT => bincode::deserialize(&res_bytes)
                    .map_err(|e| Error::SerializationError(format!("{:?}", e)))?,
                ty => { return Err(Error::InvalidPacketType(ty)); },
            };
            debug!("=> {} s (deserialize)", now.elapsed().as_secs_f32());
            Ok(res)
        }))
    }
}
