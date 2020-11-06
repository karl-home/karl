use std::collections::HashMap;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs, TcpListener};
use std::sync::{Arc, Mutex};
use std::time::{Instant, Duration};
use std::thread;

use astro_dnssd::browser::{ServiceBrowserBuilder, ServiceEventType};
use tokio::runtime::Runtime;

use protobuf::Message;
use crate::packet;
use crate::protos;
use crate::common::{Error, HT_HOST_REQUEST, HT_HOST_RESULT};

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
        let (header, _) = packet::read(&mut stream, 1)?.remove(0);
        debug!("=> {} s", now.elapsed().as_secs_f32());

        // Deploy the request to correct handler.
        let res_bytes = match header.ty {
            HT_HOST_REQUEST => {
                let host = self.find_host().unwrap();
                info!("picked host => {:?}", host);
                let mut res = protos::HostResult::default();
                res.set_ip(host.ip().to_string());
                res.set_port(host.port().into());
                res.write_to_bytes().unwrap()
            },
            ty => return Err(Error::InvalidPacketType(ty)),
        };

        // Return the result to sender.
        debug!("writing packet");
        let now = Instant::now();
        packet::write(&mut stream, HT_HOST_RESULT, &res_bytes)?;
        debug!("=> {} s", now.elapsed().as_secs_f32());
        Ok(())
    }

    /// Find a host to connect to round-robin.
    fn find_host(&mut self) -> Result<SocketAddr, Error> {
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
}
