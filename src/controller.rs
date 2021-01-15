use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs, TcpListener, Ipv4Addr, IpAddr};
use std::sync::{Arc, Mutex};
use std::time::{Instant, Duration, SystemTime, UNIX_EPOCH};
use std::thread;
use std::fs;

use serde::Serialize;
use astro_dnssd::browser::{Service, ServiceBrowserBuilder, ServiceEventType};
use tokio::runtime::Runtime;

use protobuf::Message;
use protobuf::parse_from_bytes;
use crate::dashboard;
use crate::packet;
use crate::protos;
use crate::common::{
    Error,
    HT_HOST_REQUEST, HT_HOST_RESULT, HT_REGISTER_REQUEST, HT_REGISTER_RESULT,
    HT_PING_REQUEST, HT_PING_RESULT, HT_NOTIFY_START, HT_NOTIFY_END,
};

type ServiceName = String;

/// Request information.
#[derive(Serialize, Debug, Clone)]
pub struct Request {
    /// Description of request.
    pub description: String,
    /// Request start, time since UNIX epoch.
    pub start: u64,
    /// Request end, time since UNIX epoch, or None if ongoing.
    pub end: Option<u64>,
}

/// Host status and information.
#[derive(Serialize, Debug, Clone)]
pub struct Host {
    /// Index, used internally.
    pub index: usize,
    /// Service name, as identified by DNS-SD.
    pub name: ServiceName,
    /// Host address.
    pub addr: SocketAddr,
    /// Active request.
    pub active_request: Option<Request>,
    /// Last request.
    pub last_request: Option<Request>,
}

/// Client status and information.
#[derive(Serialize, Debug, Clone)]
pub struct Client {
    /// ID, given by the client itself...
    pub id: String,
    /// IP address.
    pub addr: IpAddr,
    /// Whether the client supplied an app.
    pub app: bool,
}

/// Controller used for discovering available Karl services via DNS-SD.
///
/// Currently, each client runs its own controller, which is aware of all
/// available Karl services. Eventually, there may be a central controller
/// that coordinates client requests among available services.
/// Non-macOS services need to install the appropriate shims around DNS-SD.
pub struct Controller {
    pub rt: Runtime,
    karl_path: PathBuf,
    hosts: Arc<Mutex<Vec<ServiceName>>>,
    unique_hosts: Arc<Mutex<HashMap<ServiceName, Host>>>,
    clients: Arc<Mutex<HashMap<String, Client>>>,
    prev_host_i: usize,
}

impl Request {
    pub fn time_since_epoch_s() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
    }

    pub fn new(description: String) -> Self {
        Request {
            description,
            start: Request::time_since_epoch_s(),
            end: None,
        }
    }
}

fn add_host(
    service: &Service,
    addr: SocketAddr,
    hosts: &mut Vec<ServiceName>,
    unique_hosts: &mut HashMap<ServiceName, Host>,
) {
    if unique_hosts.contains_key(&service.name) {
        return;
    }
    info!(
        "ADD if: {} name: {} type: {} domain: {}, addr: {:?}",
        service.interface_index, service.name,
        service.regtype, service.domain, addr,
    );
    // TODO: arbitrarily take the last address
    // and hope that it is a private IP
    unique_hosts.insert(
        service.name.clone(),
        Host {
            name: service.name.clone(),
            index: hosts.len(),
            addr,
            active_request: None,
            last_request: None,
        },
    );
    hosts.push(service.name.clone());
}

fn remove_host(
    service: &Service,
    hosts: &mut Vec<ServiceName>,
    unique_hosts: &mut HashMap<ServiceName, Host>,
) {
    if let Some(host) = unique_hosts.remove(&service.name) {
        hosts.remove(host.index);
    } else {
        return;
    }
    info!(
        "RMV if: {} name: {} type: {} domain: {}",
        service.interface_index, service.name,
        service.regtype, service.domain,
    );
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
    pub fn new() -> Self {
        let c = Controller {
            rt: Runtime::new().unwrap(),
            karl_path: Path::new("/home/gina/.karl").to_path_buf(),
            hosts: Arc::new(Mutex::new(Vec::new())),
            unique_hosts: Arc::new(Mutex::new(HashMap::new())),
            clients: Arc::new(Mutex::new(HashMap::new())),
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
                        trace!("Status: {:?}", status);
                        let addrs = match r.to_socket_addrs() {
                            Ok(addrs) => addrs
                                .filter(|addr| addr.is_ipv4())
                                .filter(|addr| addr.ip() != Ipv4Addr::LOCALHOST)
                                .collect::<Vec<_>>(),
                            Err(e) => {
                                error!("Failed to resolve\n=> addrs: {:?}\n=> {:?}", r, e);
                                return;
                            },
                        };
                        if addrs.is_empty() {
                            warn!("no addresses found\n=> {:?}", r);
                            return;
                        }
                        // Update hosts with IPv4 address.
                        // Log the discovered service
                        // NOTE: only adds the first IPv4 address...
                        let mut hosts = hosts.lock().unwrap();
                        let mut unique_hosts = unique_hosts.lock().unwrap();
                        debug!("{:?}", addrs);
                        match service.event_type {
                            ServiceEventType::Added => {
                                let addr = addrs[0];
                                add_host(&service, addr, &mut hosts, &mut unique_hosts);
                            },
                            ServiceEventType::Removed => {
                                remove_host(&service, &mut hosts, &mut unique_hosts);
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
    pub fn start(
        &mut self,
        use_dashboard: bool,
        port: u16,
    ) -> Result<(), Error> {
        if use_dashboard {
            dashboard::start(
                &mut self.rt,
                self.karl_path.clone(),
                self.unique_hosts.clone(),
                self.clients.clone(),
            );
        }
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

    // Register a client's web app.
    //
    // Write the bytes to `<KARL_PATH>/www/<CLIENT_ID>.hbs`.
    fn register_app(&mut self, id: &str, app_bytes: &[u8]) {
        let parent = self.karl_path.join("www");
        let path = parent.join(format!("{}.hbs", id));
        fs::create_dir_all(parent).unwrap();
        fs::write(&path, app_bytes).unwrap();
        info!("registered app ({} bytes) at {:?}", app_bytes.len(), path);
    }

    /// Handle an incoming TCP stream.
    fn handle_client(&mut self, mut stream: TcpStream) -> Result<(), Error> {
        // Read the computation request from the TCP stream.
        let now = Instant::now();
        debug!("reading packet");
        let (req_header, req_bytes) = packet::read(&mut stream, 1)?.remove(0);
        debug!("=> {} s", now.elapsed().as_secs_f32());

        // Deploy the request to correct handler.
        match req_header.ty {
            HT_HOST_REQUEST => {
                let host = self.find_host(true).unwrap();
                info!("picked host => {:?}", host);
                let mut res = protos::HostResult::default();
                res.set_ip(host.ip().to_string());
                res.set_port(host.port().into());
                let res_bytes = res.write_to_bytes().unwrap();

                // Return the result to sender.
                debug!("writing packet ({} bytes) {:?}", res_bytes.len(), res_bytes);
                let now = Instant::now();
                packet::write(&mut stream, HT_HOST_RESULT, &res_bytes)?;
                debug!("=> {} s", now.elapsed().as_secs_f32());
            },
            HT_REGISTER_REQUEST => {
                let req = parse_from_bytes::<protos::RegisterRequest>(&req_bytes)
                    .map_err(|e| Error::SerializationError(format!("{:?}", e)))
                    .unwrap();
                let client = Client {
                    id: req.get_id().to_string(),
                    addr: stream.peer_addr().unwrap().ip(),
                    app: !req.get_app().is_empty(),
                };

                // register the client's webapp
                if client.app {
                    self.register_app(&client.id, req.get_app());
                }
                // register the client itself
                let mut clients = self.clients.lock().unwrap();
                if clients.insert(client.id.clone(), client.clone()).is_none() {
                    info!("registered client {:?}", client);
                } else {
                    warn!("client {:?} already existed!", client);
                }
                drop(clients);
                let res = protos::RegisterResult::default();
                let res_bytes = res.write_to_bytes().unwrap();

                // Return the result to sender.
                debug!("writing packet");
                let now = Instant::now();
                packet::write(&mut stream, HT_REGISTER_RESULT, &res_bytes)?;
                debug!("=> {} s", now.elapsed().as_secs_f32());
            },
            HT_NOTIFY_START => {
                let req = parse_from_bytes::<protos::NotifyStart>(&req_bytes)
                    .map_err(|e| Error::SerializationError(format!("{:?}", e)))
                    .unwrap();
                info!("notify start {:?} {:?}", req.service_name, req.description);
                let mut unique_hosts = self.unique_hosts.lock().unwrap();
                if let Some(host) = unique_hosts.get_mut(&req.service_name) {
                    if let Some(req) = &host.active_request {
                        warn!("overriding active request: {:?}", req)
                    }
                    host.active_request = Some(Request::new(req.description));
                } else {
                    warn!("missing host");
                }
            },
            HT_NOTIFY_END => {
                let req = parse_from_bytes::<protos::NotifyEnd>(&req_bytes)
                    .map_err(|e| Error::SerializationError(format!("{:?}", e)))
                    .unwrap();
                info!("notify end {:?}", req.service_name);
                let mut unique_hosts = self.unique_hosts.lock().unwrap();
                if let Some(host) = unique_hosts.get_mut(&req.service_name) {
                    if let Some(mut req) = host.active_request.take() {
                        req.end = Some(Request::time_since_epoch_s());
                        host.last_request = Some(req);
                    } else {
                        warn!("no active request, null notify end");
                    }
                } else {
                    warn!("missing host");
                }
            },
            HT_PING_REQUEST => {
                parse_from_bytes::<protos::PingRequest>(&req_bytes)
                    .map_err(|e| Error::SerializationError(format!("{:?}", e)))
                    .unwrap();
                let res = protos::PingResult::default();
                let res_bytes = res.write_to_bytes().unwrap();

                // Return the result to sender.
                debug!("writing packet {:?} ({} bytes)", res_bytes, res_bytes.len());
                let now = Instant::now();
                packet::write(&mut stream, HT_PING_RESULT, &res_bytes)?;
                debug!("=> {} s", now.elapsed().as_secs_f32());
            },
            ty => return Err(Error::InvalidPacketType(ty)),
        };
        Ok(())
    }

    /// Find a host to connect to round-robin.
    ///
    /// Returns either the host address, or an error if no hosts have
    /// registered, or all of the registered hosts have active requests.
    fn find_host(&mut self, blocking: bool) -> Result<SocketAddr, Error> {
        loop {
            let hosts = self.hosts.lock().unwrap();
            if hosts.is_empty() {
                return Err(Error::NoAvailableHosts);
            } else {
                let unique_hosts = self.unique_hosts.lock().unwrap();
                let mut host_i = self.prev_host_i;
                for i in 0..hosts.len() {
                    host_i = (host_i + 1) % hosts.len();
                    let service_name = &hosts[i];
                    let host = unique_hosts.get(service_name).unwrap();
                    if host.active_request.is_some() {
                        continue;
                    } else {
                        self.prev_host_i = host_i;
                        return Ok(host.addr);
                    }
                }
            }
            if !blocking {
                return Err(Error::NoAvailableHosts);
            }
            drop(hosts);
            thread::sleep(Duration::from_secs(1));
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use ntest::timeout;

    #[test]
    fn test_find_host_non_blocking() {
        // default controller has no hosts
        // add 3 hosts
        // find_host should return host 1 2 3 1 round robin
        // make host 3 busy
        // find_host should return 2 1 2 round robin
        // make host 1 and 2 busy
        // find_host should fail
    }

    #[test]
    #[timeout(500)]
    fn test_find_host_blocking_no_hosts() {
        // default controller has no hosts
        // unreachable statement
    }

    #[test]
    #[timeout(500)]
    fn test_find_host_blocking_unavailable() {
        // default controller has no hosts
        // add host
        // find_host returns host
        // make host busy
        // find_host blocks
        // unreachable statement
    }
}
