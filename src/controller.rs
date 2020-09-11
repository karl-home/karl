use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::time::{Instant, Duration};
use std::thread;

use bincode;
use astro_dnssd::browser::{ServiceBrowserBuilder, ServiceEventType};
use tokio::{task::JoinHandle, runtime::Runtime};

use crate::*;

/// Controller for interacting with mDNS.
pub struct Controller {
    pub rt: Runtime,
    blocking: bool,
    hosts: Arc<Mutex<Vec<SocketAddr>>>,
    prev_host_i: usize,
}

impl Controller {
    /// Create a controller that asynchronously queries for new hosts
    /// at the given interval.
    ///
    /// Parameters:
    /// - rt - Tokio runtime.
    /// - blocking - Whether the controller should block on requests.
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
            let _result = browser.start(move |result| match result {
                Ok(mut service) => {
                    // Log the discovered service
                    let event = match service.event_type {
                        ServiceEventType::Added => "ADD",
                        ServiceEventType::Removed => "RMV",
                    };
                    info!(
                        "{} if: {} name: {} type: {} domain: {}",
                        event, service.interface_index, service.name,
                        service.regtype, service.domain,
                    );
                    let results = service.resolve();
                    for r in results.unwrap() {
                        let status = r.txt_record.as_ref().unwrap().get("status");
                        let addrs_iter = r.to_socket_addrs().unwrap();
                        for addr in addrs_iter {
                            if !addr.is_ipv4() {
                                continue;
                            }
                            // Update hosts with IPv4 address.
                            debug!("Addr: {}", addr);
                            match service.event_type {
                                ServiceEventType::Added => {
                                    hosts.lock().unwrap().push(addr);
                                },
                                ServiceEventType::Removed => {
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
            });
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
    fn send(mut stream: TcpStream, req: KarlRequest) -> Result<KarlResult, Error> {
        debug!("sending {:?}...", req);
        let now = Instant::now();
        let bytes = bincode::serialize(&req)
            .map_err(|e| Error::SerializationError(format!("{:?}", e)))?;
        debug!("=> {} s (serialize)", now.elapsed().as_secs_f32());
        write_packet(&mut stream, &bytes)?;
        debug!("=> {} s (write to stream)", now.elapsed().as_secs_f32());

        // Wait for the response.
        debug!("waiting for response...");
        let now = Instant::now();
        let bytes = read_packets(&mut stream, 1)?;
        debug!("=> {} s (read from stream)", now.elapsed().as_secs_f32());
        let res = bincode::deserialize(&bytes[0])
            .map_err(|e| Error::SerializationError(format!("{:?}", e)))?;
        debug!("=> {} s (deserialize)", now.elapsed().as_secs_f32());
        Ok(res)
    }

    /// Ping the host.
    pub fn ping(&mut self) -> Result<PingResult, Error> {
        let stream = self.connect()?;
        let req = KarlRequest::Ping(PingRequest::new());
        match Controller::send(stream, req)? {
            KarlResult::Ping(res) => Ok(res),
            KarlResult::Compute(_) => Err(Error::InvalidResponseType),
        }
    }

    /// Execute a compute request.
    pub fn execute(
        &mut self,
        req: ComputeRequest,
    ) -> Result<ComputeResult, Error> {
        let stream = self.connect()?;
        let req = KarlRequest::Compute(req);
        match Controller::send(stream, req)? {
            KarlResult::Compute(res) => Ok(res),
            KarlResult::Ping(_) => Err(Error::InvalidResponseType),
        }
    }

    pub fn execute_async(
        &mut self,
        req: ComputeRequest,
    ) -> Result<JoinHandle<Result<ComputeResult, Error>>, Error> {
        let stream = self.connect()?;
        let req = KarlRequest::Compute(req);
        Ok(self.rt.spawn(async move {
            match Controller::send(stream, req)? {
                KarlResult::Compute(res) => Ok(res),
                KarlResult::Ping(_) => Err(Error::InvalidResponseType),
            }
        }))
    }
}
