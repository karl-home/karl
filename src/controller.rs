use std::collections::HashSet;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::time::{Instant, Duration};
use std::thread;

use bincode;
use astro_dnssd::browser::{ServiceBrowserBuilder, ServiceEventType};
use tokio::runtime::Runtime;

use crate::*;

/// Controller for interacting with mDNS.
pub struct Controller {
    rt: Runtime,
    blocking: bool,
    hosts: Arc<Mutex<HashSet<SocketAddr>>>,
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
            hosts: Arc::new(Mutex::new(HashSet::new())),
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
                            debug!("Addr: {}", addr);
                            if !addr.is_ipv4() {
                                continue;
                            }
                            // Update hosts with IPv4 address.
                            match service.event_type {
                                ServiceEventType::Added => {
                                    hosts.lock().unwrap().insert(addr);
                                },
                                ServiceEventType::Removed => {
                                    hosts.lock().unwrap().remove(&addr);
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

    /// Find all hosts that broadcast the service over mDNS.
    fn find_hosts(&mut self) -> Vec<SocketAddr> {
        self.hosts
            .lock()
            .unwrap()
            .clone()
            .into_iter()
            .collect::<Vec<_>>()
    }

    /// Connect to a host, returning the tcp stream.
    fn connect(&mut self, blocking: bool) -> Result<TcpStream, Error> {
        debug!("connect...");
        let now = Instant::now();
        let hosts = loop {
            let hosts = self.find_hosts();
            if !hosts.is_empty() {
                break hosts;
            }
            if !blocking {
                return Err(Error::NoAvailableHosts);
            }
            trace!("No hosts found! Try again in 1 second...");
            thread::sleep(Duration::from_secs(1));
        };
        let host = hosts[0];
        debug!("=> {} s ({:?} out of {} hosts)", now.elapsed().as_secs_f32(), host, hosts.len());
        let stream = TcpStream::connect(&host)?;
        debug!("=> {} s (connect)", now.elapsed().as_secs_f32());
        Ok(stream)
    }

    /// Send a request to the connected host.
    fn send(stream: &mut TcpStream, req: KarlRequest) -> Result<KarlResult, Error> {
        debug!("sending {:?}...", req);
        let now = Instant::now();
        let bytes = bincode::serialize(&req)
            .map_err(|e| Error::SerializationError(format!("{:?}", e)))?;
        debug!("=> {} s (serialize)", now.elapsed().as_secs_f32());
        write_packet(stream, &bytes)?;
        debug!("=> {} s (write to stream)", now.elapsed().as_secs_f32());

        // Wait for the response.
        debug!("waiting for response...");
        let now = Instant::now();
        let bytes = read_packet(stream, true)?;
        debug!("=> {} s (read from stream)", now.elapsed().as_secs_f32());
        let res = bincode::deserialize(&bytes)
            .map_err(|e| Error::SerializationError(format!("{:?}", e)))?;
        debug!("=> {} s (deserialize)", now.elapsed().as_secs_f32());
        Ok(res)
    }

    /// Ping the host.
    pub fn ping(&mut self) -> Result<PingResult, Error> {
        let mut stream = self.connect(self.blocking)?;
        let req = KarlRequest::Ping(PingRequest::new());
        match Controller::send(&mut stream, req)? {
            KarlResult::Ping(res) => Ok(res),
            KarlResult::Compute(_) => Err(Error::InvalidResponseType),
        }
    }

    /// Execute a compute request.
    pub fn execute(
        &mut self,
        req: ComputeRequest,
    ) -> Result<ComputeResult, Error> {
        let mut stream = self.connect(self.blocking)?;
        let req = KarlRequest::Compute(req);
        match Controller::send(&mut stream, req)? {
            KarlResult::Compute(res) => Ok(res),
            KarlResult::Ping(_) => Err(Error::InvalidResponseType),
        }
    }
}
