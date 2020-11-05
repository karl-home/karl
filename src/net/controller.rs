use std::collections::HashMap;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs, TcpListener};
use std::sync::{Arc, Mutex};
use std::time::{Instant, Duration};
use std::thread;

use bincode;
use astro_dnssd::browser::{ServiceBrowserBuilder, ServiceEventType};
use tokio::{task::JoinHandle, runtime::Runtime};

use crate::*;

type ServiceName = String;

/// Controller used for discovering available Karl services via DNS-SD.
///
/// Currently, each client runs its own controller, which is aware of all
/// available Karl services. Eventually, there may be a central controller
/// that coordinates client requests among available services.
/// Non-macOS services need to install the appropriate shims around DNS-SD.
pub struct Controller {
    pub rt: Runtime,
    blocking: bool,
    hosts: Arc<Mutex<Vec<ServiceName>>>,
    unique_hosts: Arc<Mutex<HashMap<ServiceName, (SocketAddr, usize)>>>,
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
    /// Call `start()` after constructing the controller to ensure it is
    /// listening for host requests.
    ///
    /// Parameters:
    /// - `blocking`: Whether the controller should block until it finds
    ///   an available host on request. Otherwise, if no hosts are available,
    ///   the controller will error.
    pub fn new(blocking: bool) -> Self {
        let c = Controller {
            rt: Runtime::new().unwrap(),
            blocking,
            hosts: Arc::new(Mutex::new(Vec::new())),
            unique_hosts: Arc::new(Mutex::new(HashMap::new())),
            prev_host_i: 0,
        };
        let hosts = c.hosts.clone();
        let unique_hosts = c.unique_hosts.clone();
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
                        debug!("Status: {:?}", status);
                        let addrs = match r.to_socket_addrs() {
                            Ok(addrs) => addrs.filter(|addr| addr.is_ipv4()).collect::<Vec<_>>(),
                            Err(e) => {
                                error!("Failed to resolve\n=> addrs: {:?}\n=> {:?}", r, e);
                                return;
                            },
                        };
                        if addrs.is_empty() {
                            error!("No addresses found: {:?}", r);
                            return;
                        }
                        // Update hosts with IPv4 address.
                        // Log the discovered service
                        // NOTE: only adds the first IPv4 address...
                        let mut hosts = hosts.lock().unwrap();
                        let mut unique_hosts = unique_hosts.lock().unwrap();
                        match service.event_type {
                            ServiceEventType::Added => {
                                if unique_hosts.contains_key(&service.name) {
                                    continue;
                                }
                                info!(
                                    "ADD if: {} name: {} type: {} domain: {} => {:?}",
                                    service.interface_index, service.name,
                                    service.regtype, service.domain, addrs,
                                );
                                unique_hosts.insert(service.name.clone(), (addrs[0], hosts.len()));
                                hosts.push(service.name.clone());
                            },
                            ServiceEventType::Removed => {
                                if let Some((_, i)) = unique_hosts.remove(&service.name) {
                                    hosts.remove(i);
                                } else {
                                    continue;
                                }
                                info!(
                                    "RMV if: {} name: {} type: {} domain: {} => {:?}",
                                    service.interface_index, service.name,
                                    service.regtype, service.domain, addrs,
                                );
                            },
                        }
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

    /// Start the TCP listener for incoming host requests
    pub fn start(&mut self, port: u16) -> Result<(), Error> {
        let listener = TcpListener::bind(format!("0.0.0.0:{}", port))?;
        info!("Karl controller listening on port {}", listener.local_addr()?.port());
        for stream in listener.incoming() {
            let stream = stream?;
            debug!("incoming stream {:?}", stream.local_addr());
            let now = Instant::now();
            if let Err(e) = self.handle_client(stream) {
                error!("{:?}", e);
            }
            warn!("total: {} s", now.elapsed().as_secs_f32());
        }
        Ok(())
    }

    /// Handle an incoming TCP stream.
    fn handle_client(&mut self, mut stream: TcpStream) -> Result<(), Error> {
        // Read the computation request from the TCP stream.
        let now = Instant::now();
        debug!("reading packet");
        let (header, _) = read_packets(&mut stream, 1)?.remove(0);
        debug!("=> {} s", now.elapsed().as_secs_f32());

        // Deploy the request to correct handler.
        let res_bytes = match header.ty {
            HT_HOST_REQUEST => {
                let host = self.find_host().unwrap();
                bincode::serialize(&HostResult {
                    ip: host.ip().to_string(),
                    port: host.port().to_string(),
                }).unwrap()
            },
            ty => return Err(Error::InvalidPacketType(ty)),
        };

        // Return the result to sender.
        debug!("writing packet");
        let now = Instant::now();
        write_packet(&mut stream, HT_HOST_RESULT, &res_bytes)?;
        debug!("=> {} s", now.elapsed().as_secs_f32());
        Ok(())
    }

    /// Find a host to connect to round-robin.
    pub fn find_host(&mut self) -> Result<SocketAddr, Error> {
        loop {
            let hosts = self.hosts.lock().unwrap();
            if !hosts.is_empty() {
                let i = (self.prev_host_i + 1) % hosts.len();
                let service_name = &hosts[i];
                self.prev_host_i = i;
                let unique_hosts = self.unique_hosts.lock().unwrap();
                return Ok(unique_hosts.get(service_name).unwrap().0);
            }
            if !self.blocking {
                return Err(Error::NoAvailableHosts);
            }
            drop(hosts);
            trace!("No hosts found! Try again in 1 second...");
            thread::sleep(Duration::from_secs(1));
        }
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
    pub fn ping(&mut self, stream: TcpStream) -> Result<PingResult, Error> {
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
        stream: TcpStream,
        req: ComputeRequest,
    ) -> Result<ComputeResult, Error> {
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
        stream: TcpStream,
        req: ComputeRequest,
    ) -> Result<JoinHandle<Result<ComputeResult, Error>>, Error> {
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
