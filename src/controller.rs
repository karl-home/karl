use std::collections::HashMap;
use std::path::PathBuf;
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
#[derive(Serialize, Debug, Clone, Default)]
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
    let removed_i = if let Some(host) = unique_hosts.remove(&service.name) {
        hosts.remove(host.index);
        host.index
    } else {
        return;
    };
    for (_, host) in unique_hosts.iter_mut() {
        if host.index > removed_i {
            host.index -= 1;
        }
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
    /// Call `start()` after constructing the controller to ensure it is
    /// listening for hosts and client requests, and that it is registered
    /// on DNS-SD.
    pub fn new(karl_path: PathBuf) -> Self {
        Controller {
            rt: Runtime::new().unwrap(),
            karl_path,
            hosts: Arc::new(Mutex::new(Vec::new())),
            unique_hosts: Arc::new(Mutex::new(HashMap::new())),
            clients: Arc::new(Mutex::new(HashMap::new())),
            prev_host_i: 0,
        }
    }

    /// Start the TCP listener for incoming host requests and spawn a process
    /// in the background that listens on DNS-SD for available hosts.
    ///
    /// In the background process, the controller maintains a list of
    /// available hosts, adding and removing hosts as specified by DNS-SD
    /// messages. Otherwise, the host listens for the following messages:
    ///
    /// - RegisterRequest: clients register themselves.
    /// - HostRequest: clients request an available host.
    /// - NotifyStart: hosts notify the controller they are unavailable.
    /// - NotifyEnd: hosts notify the controller they are available again.
    /// - PingRequest: generic ping.
    pub fn start(
        &mut self,
        use_dashboard: bool,
        port: u16,
    ) -> Result<(), Error> {
        let hosts = self.hosts.clone();
        let unique_hosts = self.unique_hosts.clone();
        debug!("Listening...");
        self.rt.spawn(async move {
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
                let res = self.find_host(true);
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
                let res = self.register_client(
                    req.get_id().to_string(),
                    stream.peer_addr().unwrap().ip(),
                    req.get_app(),
                );
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
                self.notify_start(req.service_name, req.description);
            },
            HT_NOTIFY_END => {
                let req = parse_from_bytes::<protos::NotifyEnd>(&req_bytes)
                    .map_err(|e| Error::SerializationError(format!("{:?}", e)))
                    .unwrap();
                self.notify_end(req.service_name);
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
    /// If `blocking` is true, loops in 1-second intervals until a host is
    /// available, and otherwise immediately sets `found` to false in the
    /// HostResult. A host is available if there is at least one host is
    /// registered and none of the registered hosts have active requests.
    fn find_host(&mut self, blocking: bool) -> protos::HostResult {
        let mut res = protos::HostResult::default();
        loop {
            let hosts = self.hosts.lock().unwrap();
            if !hosts.is_empty() {
                let unique_hosts = self.unique_hosts.lock().unwrap();
                let mut host_i = self.prev_host_i;
                for _ in 0..hosts.len() {
                    host_i = (host_i + 1) % hosts.len();
                    let service_name = &hosts[host_i];
                    let host = unique_hosts.get(service_name).unwrap();
                    if host.active_request.is_some() {
                        continue;
                    } else {
                        self.prev_host_i = host_i;
                        info!("picked host => {:?}", host.addr);
                        res.set_ip(host.addr.ip().to_string());
                        res.set_port(host.addr.port().into());
                        res.set_found(true);
                        return res;
                    }
                }
            }
            if !blocking {
                return res;
            }
            drop(hosts);
            thread::sleep(Duration::from_secs(1));
        }
    }

    /// Register a client.
    ///
    /// Stores the client-generated ID along with socket information about
    /// the client. Overwrites duplicate client IDs. Registers the app
    /// (a Handlebars template) at `<KARL_PATH>/www/<CLIENT_ID>.hbs`. Creates
    /// an empty storage directory at `<KARL_PATH>/storage/<CLIENT_ID>/`,
    /// if it doesn't already exist.
    ///
    /// Parameters:
    /// - client_id - The client-generated ID.
    /// - client_addr - The peer address of the TCP connection registering
    ///   the client.
    /// - app_bytes - The bytes of the Handlebars template, or an empty
    ///   vector if there is no registered app.
    fn register_client(
        &mut self,
        client_id: String,
        client_addr: IpAddr,
        app_bytes: &[u8],
    ) -> protos::RegisterResult {
        let client = Client {
            id: client_id,
            addr: client_addr,
            app: !app_bytes.is_empty(),
        };

        // register the client's webapp
        if client.app {
            let parent = self.karl_path.join("www");
            let path = parent.join(format!("{}.hbs", &client.id));
            fs::create_dir_all(parent).unwrap();
            fs::write(&path, app_bytes).unwrap();
            info!("registered app ({} bytes) at {:?}", app_bytes.len(), path);
        }

        // create a storage directory
        let storage_path = self.karl_path.join("storage").join(&client.id);
        fs::create_dir_all(storage_path).unwrap();

        // register the client itself
        let mut clients = self.clients.lock().unwrap();
        if clients.insert(client.id.clone(), client.clone()).is_none() {
            info!("registered client {:?}", client);
        } else {
            warn!("client {:?} already existed!", client);
        }
        protos::RegisterResult::default()
    }

    /// Notify the controller that a service is starting a request.
    ///
    /// Finds the host with the given service name and sets the active
    /// request to the given description. Logs an error message if the host
    /// cannot be found, or an already active request is overwritten.
    fn notify_start(&mut self, service_name: String, description: String) {
        info!("notify start {:?} {:?}", service_name, description);
        let mut unique_hosts = self.unique_hosts.lock().unwrap();
        if let Some(host) = unique_hosts.get_mut(&service_name) {
            if let Some(req) = &host.active_request {
                error!("overriding active request: {:?}", req)
            }
            host.active_request = Some(Request::new(description));
        } else {
            error!("missing host");
        }
    }

    /// Notify the controller that a service is ending a request.
    ///
    /// Finds the host with the given service name and sets the last request
    /// to be the previously active request, updating the end time. Logs an
    /// error message if the host cannot be found, or if the host does not
    /// have an active request.
    fn notify_end(&mut self, service_name: String) {
        info!("notify end {:?}", service_name);
        let mut unique_hosts = self.unique_hosts.lock().unwrap();
        if let Some(host) = unique_hosts.get_mut(&service_name) {
            if let Some(mut req) = host.active_request.take() {
                req.end = Some(Request::time_since_epoch_s());
                host.last_request = Some(req);
            } else {
                error!("no active request, null notify end");
            }
        } else {
            error!("missing host");
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use ntest::timeout;
    use tempdir::TempDir;

    /// Create a temporary directory.
    fn init_karl_path() -> TempDir {
        TempDir::new("karl").unwrap()
    }

    /// Generate a DNS-SD service with the name "host<i>".
    fn gen_service(i: usize) -> Service {
        Service {
            name: format!("host{}", i),
            regtype: "".to_string(),
            interface_index: 0,
            domain: "".to_string(),
            event_type: ServiceEventType::Added,
        }
    }

    /// Add a host named "host<i>" with socket addr "0.0.0.0:808<i>".
    fn add_host_test(
        i: usize,
        hosts: &mut Vec<ServiceName>,
        unique_hosts: &mut HashMap<ServiceName, Host>,
    ) {
        let service = gen_service(i);
        let addr: SocketAddr = format!("0.0.0.0:808{}", i).parse().unwrap();
        add_host(&service, addr, hosts, unique_hosts);
    }

    #[test]
    fn test_add_host() {
        let karl_path = init_karl_path();
        let c = Controller::new(karl_path.path().to_path_buf());
        let mut hosts = c.hosts.lock().unwrap();
        let mut unique_hosts = c.unique_hosts.lock().unwrap();
        assert!(hosts.is_empty());
        assert!(unique_hosts.is_empty());

        // Add a host
        let name = "host1";
        let addr: SocketAddr = "127.0.0.1:8081".parse().unwrap();
        add_host(&gen_service(1), addr, &mut hosts, &mut unique_hosts);

        // Check controller was modified correctly
        assert_eq!(hosts.len(), 1);
        assert_eq!(unique_hosts.len(), 1);
        assert_eq!(hosts[0], name, "wrong service name");
        assert!(unique_hosts.get(name).is_some(), "violated host service name invariant");

        // Check host was initialized correctly
        let host = unique_hosts.get(name).unwrap();
        assert_eq!(host.index, 0);
        assert_eq!(host.name, name);
        assert_eq!(host.addr, addr);
        assert!(host.active_request.is_none());
        assert!(host.last_request.is_none());
    }

    #[test]
    fn test_add_multiple_hosts() {
        let karl_path = init_karl_path();
        let c = Controller::new(karl_path.path().to_path_buf());
        let mut hosts = c.hosts.lock().unwrap();
        let mut unique_hosts = c.unique_hosts.lock().unwrap();

        // Add three hosts
        add_host_test(1, &mut hosts, &mut unique_hosts);
        add_host_test(2, &mut hosts, &mut unique_hosts);
        add_host_test(3, &mut hosts, &mut unique_hosts);

        // Check the index in unique_hosts corresponds to the index in hosts
        for (name, host) in unique_hosts.iter() {
            assert_eq!(name, &host.name);
            assert_eq!(hosts[host.index], host.name);
        }
    }

    #[test]
    fn test_remove_host() {
        let karl_path = init_karl_path();
        let c = Controller::new(karl_path.path().to_path_buf());
        let mut hosts = c.hosts.lock().unwrap();
        let mut unique_hosts = c.unique_hosts.lock().unwrap();

        // Add hosts
        add_host_test(1, &mut hosts, &mut unique_hosts);
        add_host_test(2, &mut hosts, &mut unique_hosts);
        add_host_test(3, &mut hosts, &mut unique_hosts);
        add_host_test(4, &mut hosts, &mut unique_hosts);

        // Remove the last host
        let service = gen_service(4);
        assert!(hosts.contains(&"host4".to_string()));
        remove_host(&service, &mut hosts, &mut unique_hosts);
        assert!(!hosts.contains(&"host4".to_string()));
        assert_eq!(hosts.len(), 3);
        assert_eq!(unique_hosts.len(), 3);
        for host in unique_hosts.values() {
            assert_eq!(hosts[host.index], host.name);
        }

        // Remove the middle host
        let service = gen_service(2);
        assert!(hosts.contains(&"host2".to_string()));
        remove_host(&service, &mut hosts, &mut unique_hosts);
        assert!(!hosts.contains(&"host2".to_string()));
        assert_eq!(hosts.len(), 2);
        assert_eq!(unique_hosts.len(), 2);
        for host in unique_hosts.values() {
            assert_eq!(hosts[host.index], host.name);
        }

        // Remove the first host
        let service = gen_service(1);
        assert!(hosts.contains(&"host1".to_string()));
        remove_host(&service, &mut hosts, &mut unique_hosts);
        assert!(!hosts.contains(&"host1".to_string()));
        assert_eq!(hosts.len(), 1);
        assert_eq!(unique_hosts.len(), 1);
        for host in unique_hosts.values() {
            assert_eq!(hosts[host.index], host.name);
        }
    }

    #[test]
    fn test_find_host_non_blocking() {
        let karl_path = init_karl_path();
        let mut c = Controller::new(karl_path.path().to_path_buf());

        // Add three hosts
        add_host_test(1, &mut c.hosts.lock().unwrap(), &mut c.unique_hosts.lock().unwrap());
        add_host_test(2, &mut c.hosts.lock().unwrap(), &mut c.unique_hosts.lock().unwrap());
        add_host_test(3, &mut c.hosts.lock().unwrap(), &mut c.unique_hosts.lock().unwrap());
        assert_eq!(c.hosts.lock().unwrap().clone(), vec![
            "host1".to_string(),
            "host2".to_string(),
            "host3".to_string(),
        ]);

        // Set last_request of a host, say host 2.
        // find_host returns 2 3 1 round-robin.
        c.unique_hosts.lock().unwrap().get_mut("host2").unwrap().last_request = Some(Request::default());
        let host = c.find_host(false);
        assert!(host.get_found());
        assert_eq!(host.get_port(), 8082);
        let host = c.find_host(false);
        assert!(host.get_found());
        assert_eq!(host.get_port(), 8083);
        let host = c.find_host(false);
        assert!(host.get_found());
        assert_eq!(host.get_port(), 8081);

        // Make host 3 busy.
        // find_host should return 2 1 2 round-robin.
        c.unique_hosts.lock().unwrap().get_mut("host3").unwrap().active_request = Some(Request::default());
        let host = c.find_host(false);
        assert!(host.get_found());
        assert_eq!(host.get_port(), 8082);
        let host = c.find_host(false);
        assert!(host.get_found());
        assert_eq!(host.get_port(), 8081);
        let host = c.find_host(false);
        assert!(host.get_found());
        assert_eq!(host.get_port(), 8082);

        // Make host 1 and 2 busy.
        // find_host should fail.
        c.unique_hosts.lock().unwrap().get_mut("host1").unwrap().active_request = Some(Request::default());
        c.unique_hosts.lock().unwrap().get_mut("host2").unwrap().active_request = Some(Request::default());
        let host = c.find_host(false);
        assert!(!host.get_found());
    }

    /// If the test times out, it actually succeeds!
    #[test]
    #[ignore]
    #[timeout(500)]
    fn test_find_host_blocking_no_hosts() {
        let karl_path = init_karl_path();
        let mut c = Controller::new(karl_path.path().to_path_buf());
        let blocking = true;
        c.find_host(blocking);
        unreachable!("default controller should not return without hosts");
    }

    /// If the test times out, it actually succeeds!
    #[test]
    #[ignore]
    #[timeout(500)]
    fn test_find_host_blocking_unavailable() {
        let karl_path = init_karl_path();
        let mut c = Controller::new(karl_path.path().to_path_buf());

        // Add a host.
        add_host_test(1, &mut c.hosts.lock().unwrap(), &mut c.unique_hosts.lock().unwrap());
        assert!(c.find_host(true).get_found());
        assert!(c.find_host(true).get_found());

        // Now make it busy.
        c.unique_hosts.lock().unwrap().get_mut("host1").unwrap().active_request = Some(Request::default());
        c.find_host(true);
        unreachable!("default controller should not return with busy hosts");
    }

    #[test]
    fn test_register_client() {
        let karl_path = init_karl_path();
        let mut c = Controller::new(karl_path.path().to_path_buf());
        let storage_path = karl_path.path().join("storage").join("hello");
        let app_path = karl_path.path().join("www").join("hello.hbs");

        // Check initial conditions.
        assert!(c.clients.lock().unwrap().is_empty());
        assert!(!storage_path.exists());
        assert!(!app_path.exists());

        // Register a client with an app.
        let client_id = "hello";
        let client_ip: IpAddr = "127.0.0.1".parse().unwrap();
        let app = vec![10, 10, 10, 10];
        c.register_client(client_id.to_string(), client_ip, &app);
        assert_eq!(c.clients.lock().unwrap().len(), 1, "registered client");

        // Check the client was registered correctly.
        let client = c.clients.lock().unwrap().get(client_id).unwrap().clone();
        assert_eq!(client.id, client_id);
        assert_eq!(client.addr, client_ip);
        assert!(client.app);
        assert!(storage_path.is_dir(), "storage dir was not created");
        assert!(app_path.is_file(), "app was not created and renamed");

        // Register a client without an app.
        // Storage directory should be created, but app file should not.
        let app: Vec<u8> = vec![];
        c.register_client("world".to_string(), client_ip, &app);
        assert_eq!(c.clients.lock().unwrap().len(), 2);
        assert!(karl_path.path().join("storage").join("world").is_dir());
        assert!(!karl_path.path().join("www").join("world.hbs").exists());
    }

    #[test]
    fn test_notify_start() {
        let karl_path = init_karl_path();
        let mut c = Controller::new(karl_path.path().to_path_buf());

        // Notify start with no hosts. Nothing errors.
        let service_name = "host1".to_string();
        let description = "my first app :)";
        c.notify_start(service_name.clone(), description.to_string());

        // Create a host and notify start.
        add_host_test(1, &mut c.hosts.lock().unwrap(), &mut c.unique_hosts.lock().unwrap());
        assert!(c.unique_hosts.lock().unwrap().get("host1").unwrap().active_request.is_none(),
            "no initial active request");
        c.notify_start(service_name.clone(), description.to_string());
        let request = c.unique_hosts.lock().unwrap().get("host1").unwrap().active_request.clone();
        assert!(request.is_some(), "active request started");
        let request = request.unwrap();
        assert_eq!(request.description, description, "same description");
        assert!(request.start > 0, "request has a start time");
        assert!(request.end.is_none(), "request does not have an end time");

        // Notify start again and overwrite the old request.
        thread::sleep(Duration::from_secs(2));
        c.notify_start(service_name.clone(), "what??".to_string());
        let new_request = c.unique_hosts.lock().unwrap().get("host1").unwrap().active_request.clone();
        assert!(new_request.is_some());
        let new_request = new_request.unwrap();
        assert!(new_request.description != description, "description changed");
        assert!(new_request.start > request.start, "start time increased");
        assert!(new_request.end.is_none(), "end time still does not exist");
    }

    #[test]
    fn test_notify_end() {
        let karl_path = init_karl_path();
        let mut c = Controller::new(karl_path.path().to_path_buf());

        // Notify start with no hosts. Nothing errors.
        let service_name = "host1".to_string();
        let description = "description".to_string();
        c.notify_end(service_name.clone());

        // Create a host. Notify end does not do anything without an active request.
        add_host_test(1, &mut c.hosts.lock().unwrap(), &mut c.unique_hosts.lock().unwrap());
        assert!(c.unique_hosts.lock().unwrap().get("host1").unwrap().active_request.is_none());
        assert!(c.unique_hosts.lock().unwrap().get("host1").unwrap().last_request.is_none());
        c.notify_end(service_name.clone());
        assert!(c.unique_hosts.lock().unwrap().get("host1").unwrap().active_request.is_none());
        assert!(c.unique_hosts.lock().unwrap().get("host1").unwrap().last_request.is_none());

        // Notify start then notify end.
        c.notify_start(service_name.clone(), description.clone());
        thread::sleep(Duration::from_secs(2));
        c.notify_end(service_name.clone());
        assert!(c.unique_hosts.lock().unwrap().get("host1").unwrap().active_request.is_none());
        assert!(c.unique_hosts.lock().unwrap().get("host1").unwrap().last_request.is_some());
        let request = c.unique_hosts.lock().unwrap().get("host1").unwrap().last_request.clone().unwrap();
        assert!(request.description == description);
        assert!(request.end.is_some());
        assert!(request.end.unwrap() > request.start);
    }
}
