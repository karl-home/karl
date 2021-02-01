use std::collections::{HashSet, HashMap};
use std::path::PathBuf;
use std::net::{SocketAddr, TcpStream, TcpListener, IpAddr};
#[cfg(feature = "dnssd")]
use std::net::{ToSocketAddrs, Ipv4Addr};
use std::sync::{Arc, Mutex};
use std::time::{Instant, Duration};
use std::thread;
use std::fs;
use std::io::Read;

use serde::{Serialize, ser::{Serializer, SerializeStruct}};
#[cfg(feature = "dnssd")]
use astro_dnssd::browser::{ServiceBrowserBuilder, ServiceEventType};
use tokio::runtime::Runtime;

use protobuf::Message;
use protobuf::parse_from_bytes;
use crate::dashboard;
use crate::packet;
use crate::protos;
use crate::common::{
    Error, Token, ClientToken, RequestToken,
    HT_HOST_REQUEST, HT_HOST_RESULT, HT_REGISTER_REQUEST, HT_REGISTER_RESULT,
    HT_PING_REQUEST, HT_PING_RESULT, HT_NOTIFY_START, HT_NOTIFY_END,
    HT_HOST_HEARTBEAT, HT_HOST_REGISTER_REQUEST,
};

type ServiceName = String;

/// Request information.
#[derive(Debug, Clone)]
pub struct Request {
    /// Description of request.
    pub description: String,
    /// Request start time.
    pub start: Instant,
    /// Request end time.
    pub end: Option<Instant>,
}

/// Host status and information.
#[derive(Debug, Clone)]
pub struct Host {
    /// Whether the user has confirmed this host.
    pub confirmed: bool,
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
    /// Request token. If there is no token, the host has yet to contact the
    /// controller, or the controller has already allocated the token to a
    /// client.
    pub token: Option<RequestToken>,
    /// Time of last heartbeat, notify start, or notify end.
    pub last_msg: Instant,
    /// Total number of requests handled.
    pub total: usize,
}

/// Client status and information.
#[derive(Serialize, Debug, Clone)]
pub struct Client {
    /// Whether the user has confirmed this client.
    pub confirmed: bool,
    /// The self-given lowercase alphanumeric and underscore name of the client,
    /// with _1, _2, etc. appended when duplicates are registered, like handling
    /// duplicates in the filesystem.
    pub name: String,
    /// IP address for proxy requests.
    pub addr: IpAddr,
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
    /// The second argument is prev_host_i used to index the next host.
    hosts: Arc<Mutex<(Vec<ServiceName>, usize)>>,
    unique_hosts: Arc<Mutex<HashMap<ServiceName, Host>>>,
    /// Map from client token to client.
    ///
    /// Unique identifier for the client, known only by the controller
    /// and the client itself. Generated on registration. All host
    /// requests from the client to the controller must include this token.
    clients: Arc<Mutex<HashMap<ClientToken, Client>>>,
    /// Password required for a host to register with the controller.
    password: String,
    /// Whether to automatically confirm clients and hosts.
    autoconfirm: bool,
}

impl Serialize for Host {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Host", 9)?;
        state.serialize_field("confirmed", &self.confirmed)?;
        state.serialize_field("index", &self.index)?;
        state.serialize_field("name", &self.name)?;
        state.serialize_field("addr", &self.addr)?;
        state.serialize_field("active_request", &self.active_request)?;
        state.serialize_field("last_request", &self.last_request)?;
        state.serialize_field("token", &self.token)?;
        state.serialize_field("last_msg", &self.last_msg.elapsed().as_secs_f32())?;
        state.serialize_field("total", &self.total)?;
        state.end()
    }
}

impl Serialize for Request {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let time = if let Some(end) = self.end {
            (end - self.start).as_secs_f32()
        } else {
            self.start.elapsed().as_secs_f32()
        };
        let mut state = serializer.serialize_struct("Request", 2)?;
        state.serialize_field("description", &self.description)?;
        state.serialize_field("time", &time)?;
        state.end()
    }
}

impl Default for Request {
    fn default() -> Self {
        Request::new("".to_string())
    }
}

impl Request {
    pub fn new(description: String) -> Self {
        Request {
            description,
            start: Instant::now(),
            end: None,
        }
    }
}

/// Add a host. If a host with the same name already exists, don't do anything.
///
/// Parameters:
/// - name - The name of the service.
/// - addr - The address of the host.
/// - hosts - List of host service names.
/// - unique_hosts - Hash map of host service names to host information
///   to prevent duplication.
/// - confirmed - Whether the host should be confirmed by default.
///
/// Returns:
/// Whether the host was added. Not added if it is a duplicate by name.
fn add_host(
    name: &str,
    addr: SocketAddr,
    hosts: &mut Vec<ServiceName>,
    unique_hosts: &mut HashMap<ServiceName, Host>,
    confirmed: bool,
) -> bool {
    if unique_hosts.contains_key(name) {
        return false;
    }
    // TODO: arbitrarily take the last address
    // and hope that it is a private IP
    info!("ADDED host {:?} {:?}", name, addr);
    unique_hosts.insert(
        name.to_string(),
        Host {
            confirmed,
            name: name.to_string(),
            index: hosts.len(),
            addr,
            active_request: None,
            last_request: None,
            token: None,
            last_msg: Instant::now(),
            total: 0,
        },
    );
    hosts.push(name.to_string());
    true
}

/// Returns:
/// Whether the host was removed.
fn remove_host(
    name: &str,
    hosts: &mut Vec<ServiceName>,
    unique_hosts: &mut HashMap<ServiceName, Host>,
) -> bool {
    let removed_i = if let Some(host) = unique_hosts.remove(name) {
        hosts.remove(host.index);
        host.index
    } else {
        return false;
    };
    info!("REMOVED host {:?}", name);
    for (_, host) in unique_hosts.iter_mut() {
        if host.index > removed_i {
            host.index -= 1;
        }
    }
    true
}

impl Controller {
    /// Create a new controller.
    ///
    /// Call `start()` after constructing the controller to ensure it is
    /// listening for hosts and client requests.
    pub fn new(karl_path: PathBuf, password: &str, autoconfirm: bool) -> Self {
        Controller {
            rt: Runtime::new().unwrap(),
            karl_path,
            hosts: Arc::new(Mutex::new((Vec::new(), 0))),
            unique_hosts: Arc::new(Mutex::new(HashMap::new())),
            clients: Arc::new(Mutex::new(HashMap::new())),
            password: password.to_string(),
            autoconfirm,
        }
    }

    /// Start the TCP listener for incoming host requests and spawn a process
    /// in the background that listens on DNS-SD for available hosts.
    /// Initializes clients based on the `<KARL_PATH>/clients.txt` file.
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
        port: u16,
    ) -> Result<(), Error> {
        // Make the karl path if it doesn't already exist.
        fs::create_dir_all(&self.karl_path).unwrap();

        #[cfg(feature = "dnssd")]
        {
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
                                if add_host(&service.name, addr, &mut hosts.0, &mut unique_hosts, false) {
                                    info!(
                                        "ADD if: {} name: {} type: {} domain: {}, addr: {:?}",
                                        service.interface_index, service.name,
                                        service.regtype, service.domain, addr,
                                    );
                                }
                            },
                            ServiceEventType::Removed => {
                                if remove_host(&service.name, &mut hosts.0, &mut unique_hosts) {
                                    info!(
                                        "RMV if: {} name: {} type: {} domain: {}",
                                        service.interface_index, service.name,
                                        service.regtype, service.domain,
                                    );
                                }
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
        }

        // Initialize clients in the clients file at `<KARL_PATH>/clients.txt`.
        // Expect client serialization format based on `dashboard/mod.rs`:
        // `<CLIENT_NAME>:<CLIENT_ADDR>=<CLIENT_TOKEN>`
        {
            let mut clients = self.clients.lock().unwrap();
            let path = self.karl_path.join("clients.txt");
            debug!("initializing clients at {:?}", &path);
            let mut buffer = String::new();
            let mut file = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(&path)
                .expect("unable to open clients file");
            file.read_to_string(&mut buffer).expect("failed to read file");
            for line in buffer.split('\n') {
                if line.is_empty() {
                    continue;
                }
                let i = line.find(":").expect("malformed clients.txt");
                let j = line.find("=").expect("malformed clients.txt");
                assert!(i < j, "malformed clients.txt");
                clients.insert(
                    Token(line[(j+1)..].to_string()),
                    Client {
                        confirmed: true,
                        name: line[..i].to_string(),
                        addr: line[(i+1)..j].parse().unwrap(),
                    }
                );
            }
        }

        // Start the dashboard.
        dashboard::start(
            &mut self.rt,
            self.karl_path.clone(),
            self.unique_hosts.clone(),
            self.clients.clone(),
        );
        let listener = TcpListener::bind(format!("0.0.0.0:{}", port))?;
        info!("Karl controller listening on port {}", listener.local_addr()?.port());
        for stream in listener.incoming() {
            let stream = stream?;
            trace!("incoming stream {:?}", stream.local_addr());
            let now = Instant::now();
            if let Err(e) = self.handle_client(stream) {
                error!("{:?}", e);
            }
            info!("total: {} s", now.elapsed().as_secs_f32());
        }
        Ok(())
    }

    /// Handle an incoming TCP stream.
    fn handle_client(&mut self, mut stream: TcpStream) -> Result<(), Error> {
        // Read the computation request from the TCP stream.
        let now = Instant::now();
        trace!("reading packet");
        let (req_header, req_bytes) = packet::read(&mut stream, 1)?.remove(0);
        trace!("=> {} s", now.elapsed().as_secs_f32());

        // Deploy the request to correct handler.
        match req_header.ty {
            HT_HOST_REQUEST => {
                let req = parse_from_bytes::<protos::HostRequest>(&req_bytes)
                    .map_err(|e| Error::SerializationError(format!("{:?}", e)))
                    .unwrap();
                let hosts = self.hosts.clone();
                let unique_hosts = self.unique_hosts.clone();
                let clients = self.clients.clone();
                if req.blocking {
                    self.rt.spawn(async move {
                        let res = Self::find_host(
                            hosts,
                            unique_hosts,
                            clients,
                            &Token(req.client_token),
                            req.blocking,
                        );
                        let res_bytes = res.write_to_bytes().unwrap();

                        // Return the result to sender.
                        trace!("writing packet ({} bytes) {:?}", res_bytes.len(), res_bytes);
                        let now = Instant::now();
                        packet::write(&mut stream, HT_HOST_RESULT, &res_bytes).unwrap();
                        trace!("=> {} s", now.elapsed().as_secs_f32());
                    });
                } else {
                    let res = Self::find_host(
                        hosts,
                        unique_hosts,
                        clients,
                        &Token(req.client_token),
                        req.blocking,
                    );
                    let res_bytes = res.write_to_bytes().unwrap();

                    // Return the result to sender.
                    trace!("writing packet ({} bytes) {:?}", res_bytes.len(), res_bytes);
                    let now = Instant::now();
                    packet::write(&mut stream, HT_HOST_RESULT, &res_bytes)?;
                    trace!("=> {} s", now.elapsed().as_secs_f32());
                }
            },
            HT_REGISTER_REQUEST => {
                let req = parse_from_bytes::<protos::RegisterRequest>(&req_bytes)
                    .map_err(|e| Error::SerializationError(format!("{:?}", e)))
                    .unwrap();
                let res = self.register_client(
                    req.get_id().to_string(),
                    stream.peer_addr().unwrap().ip(),
                    req.get_app(),
                    false,
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
                let ip = stream.peer_addr().unwrap().ip();
                self.verify_host_name(&req.service_name, &ip)?;
                self.notify_start(req.service_name, req.description);
            },
            HT_NOTIFY_END => {
                let req = parse_from_bytes::<protos::NotifyEnd>(&req_bytes)
                    .map_err(|e| Error::SerializationError(format!("{:?}", e)))
                    .unwrap();
                let ip = stream.peer_addr().unwrap().ip();
                self.verify_host_name(&req.service_name, &ip)?;
                self.notify_end(req.service_name, Token(req.request_token));
            },
            HT_HOST_HEARTBEAT => {
                let req = parse_from_bytes::<protos::HostHeartbeat>(&req_bytes)
                    .map_err(|e| Error::SerializationError(format!("{:?}", e)))
                    .unwrap();
                let ip = stream.peer_addr().unwrap().ip();
                self.verify_host_name(&req.service_name, &ip)?;
                self.heartbeat(req.service_name, &req.request_token);
            },
            HT_HOST_REGISTER_REQUEST => {
                let req = parse_from_bytes::<protos::HostRegisterRequest>(&req_bytes)
                    .map_err(|e| Error::SerializationError(format!("{:?}", e)))
                    .unwrap();
                let ip = stream.peer_addr().unwrap().ip();
                if !self.add_host(
                    &req.service_name,
                    format!("{}:{}", req.ip, req.port).parse().unwrap(),
                    false,
                    req.get_password(),
                ) {
                    warn!("failed to register {} ({:?})", &req.service_name, ip);
                }
                self.verify_host_name(&req.service_name, &ip)?;
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

    /// Add a host. If a host with the same name already exists, don't do anything.
    ///
    /// Parameters:
    /// - name - The name of the service.
    /// - addr - The address of the host.
    /// - confirmed - Whether the host should be confirmed by default.
    /// - password - Controller password known by the host.
    ///
    /// Returns:
    /// Whether the host was added. Not added if it is a duplicate by name,
    /// or if the password was incorrect.
    fn add_host(
        &mut self,
        name: &str,
        addr: SocketAddr,
        confirmed: bool,
        password: &str,
    ) -> bool {
        let mut hosts = self.hosts.lock().unwrap();
        let mut unique_hosts = self.unique_hosts.lock().unwrap();
        if password != self.password {
            warn!("incorrect password from {} ({:?})", &name, addr);
            false
        } else {
            add_host(
                name,
                addr,
                &mut hosts.0,
                &mut unique_hosts,
                confirmed || self.autoconfirm,
            )
        }
    }

    /// Removes the host.
    ///
    /// Parameters:
    /// - name - The name of the service.
    ///
    /// Returns:
    /// Whether the host was removed.
    fn remove_host(
        &mut self,
        name: &str,
    ) -> bool {
        let mut hosts = self.hosts.lock().unwrap();
        let mut unique_hosts = self.unique_hosts.lock().unwrap();
        remove_host(name, &mut hosts.0, &mut unique_hosts)
    }

    /// Verify messages from a host actually came from the host.
    ///
    /// The `service_name` is the name of the host provided in messages
    /// of the following types: HostHeartbeat, NotifyStart, NotifyEnd.
    /// The IP address is the address of the incoming TCP stream.
    ///
    /// The addresses _may_ not be the same as the ones retrieved from DNS-SD.
    /// It may be that multiple IP addresses resolve to the same host. For this
    /// reason, we allow requests from localhost, assuming the user can secure
    /// the host that runs the controller... Also assumes TCP connections
    /// can't spoof the IP address of the peer address, and that the host
    /// machines are similarly not compromised. If one of the host messages
    /// makes it past these simple layers of defense, the worst that happens
    /// should be just request tokens are kind of messed up and clients can't
    /// make requests.
    ///
    /// Returns: Ok if the IP addresses are the same, and an error if the IP
    /// addresses are different or a host with the name does not exist.
    fn verify_host_name(
        &self,
        service_name: &str,
        stream_ip: &IpAddr,
    ) -> Result<(), Error> {
        let ip = self.unique_hosts.lock().unwrap().iter()
            .map(|(_, host)| host)
            .filter(|host| &host.name == service_name)
            .map(|host| host.addr.ip())
            .collect::<Vec<_>>();
        if ip.is_empty() {
            return Err(Error::InvalidHostMessage(format!(
                "failed to find host with name => {}", service_name)));
        }
        if ip.len() > 1 {
            return Err(Error::InvalidHostMessage(format!(
                "found multiple hosts with name => {}", service_name)));
        }

        let allowed_ips = vec![
            ip[0],
            "127.0.0.1".parse().unwrap(),
            "0.0.0.0".parse().unwrap(),
        ];
        if !allowed_ips.contains(stream_ip) {
            return Err(Error::InvalidHostMessage(format!(
                "expected ip {:?} for host {}, received => {:?}",
                ip[0], service_name, stream_ip)));
        }
        Ok(())
    }

    /// Find a host to connect to round-robin.
    ///
    /// If the provided client token does not correspond to a registered
    /// client, logs a warning message about an unauthorized client and
    /// returns no hosts found, even if hosts are available.
    ///
    /// If `blocking` is true, loops in 1-second intervals until a host is
    /// available, and otherwise immediately sets `found` to false in the
    /// HostResult. A host is available if there is at least one host is
    /// registered and none of the registered hosts have active requests.
    fn find_host(
        hosts: Arc<Mutex<(Vec<ServiceName>, usize)>>,
        unique_hosts: Arc<Mutex<HashMap<ServiceName, Host>>>,
        clients: Arc<Mutex<HashMap<ClientToken, Client>>>,
        token: &ClientToken,
        blocking: bool,
    ) -> protos::HostResult {
        // Validate the client token.
        let mut res = protos::HostResult::default();
        if let Some(client) = clients.lock().unwrap().get(token) {
            if !client.confirmed {
                println!("find_host unconfirmed client token {:?}", token);
                return res;
            }
        } else {
            println!("find_host invalid client token {:?}", token);
            return res;
        }

        // Find a host.
        loop {
            let mut hosts = hosts.lock().unwrap();
            if !hosts.0.is_empty() {
                let mut unique_hosts = unique_hosts.lock().unwrap();
                let mut host_i = hosts.1;
                for _ in 0..hosts.0.len() {
                    host_i = (host_i + 1) % hosts.0.len();
                    let service_name = &hosts.0[host_i];
                    let host = unique_hosts.get_mut(service_name).unwrap();
                    if host.active_request.is_some() || !host.confirmed {
                        continue;
                    }
                    let elapsed = host.last_msg.elapsed().as_secs();
                    if elapsed > 2 * crate::host::HEARTBEAT_INTERVAL {
                        continue;
                    }
                    if let Some(token) = host.token.take() {
                        hosts.1 = host_i;
                        println!("find_host picked => {:?}", host.addr);
                        res.set_ip(host.addr.ip().to_string());
                        res.set_port(host.addr.port().into());
                        res.set_request_token(token.0);
                        res.set_found(true);
                        return res;
                    }
                }
            }
            if !blocking {
                println!("find_host no hosts available {:?}", token);
                return res;
            }
            drop(hosts);
            thread::sleep(Duration::from_secs(1));
        }
    }

    /// Register a client.
    ///
    /// Stores the client-generated name and controller-generated token
    /// along with socket information about the client. Registers the app
    /// (a Handlebars template) at `<KARL_PATH>/www/<CLIENT_ID>.hbs`. Creates
    /// an empty storage directory at `<KARL_PATH>/storage/<CLIENT_ID>/`,
    /// if it doesn't already exist.
    ///
    /// All characters in the client name must be lowercase alphabet a-z,
    /// digits 0-9, or underscores. Lowercases the client name, and removes
    /// other noncomplying characters. For example, "a @#Ld_e" becomes "alde".
    /// Resolves duplicate client names by appending an underscore and a number.
    /// e.g. camera -> camera_1 -> camera_2.
    ///
    /// Parameters:
    /// - name - The self-given name of the client.
    /// - client_addr - The peer address of the TCP connection registering
    ///   the client.
    /// - app_bytes - The bytes of the Handlebars template, or an empty
    ///   vector if there is no registered app.
    /// - confirmed - Whether the client should be confirmed by default.
    ///   Overriden by autoconfirm.
    fn register_client(
        &mut self,
        mut name: String,
        client_addr: IpAddr,
        app_bytes: &[u8],
        confirmed: bool,
    ) -> protos::RegisterResult {
        // resolve duplicate client names
        let names = self.clients.lock().unwrap().values()
            .map(|client| client.name.clone()).collect::<HashSet<_>>();
        name = name.trim().to_lowercase();
        name = name
            .chars()
            .filter(|ch| ch.is_alphanumeric() || ch == &'_')
            .collect();
        if names.contains(&name) {
            let mut i = 1;
            loop {
                let new_name = format!("{}_{}", name, i);
                if !names.contains(&new_name) {
                    name = new_name;
                    break;
                }
                i += 1;
            }
        }

        // generate a client with a unique name and token
        let client = Client {
            confirmed: confirmed || self.autoconfirm,
            name,
            addr: client_addr,
        };

        // register the client's webapp
        if !app_bytes.is_empty() {
            let parent = self.karl_path.join("www");
            let path = parent.join(format!("{}.hbs", &client.name));
            fs::create_dir_all(parent).unwrap();
            fs::write(&path, app_bytes).unwrap();
            info!("registered app ({} bytes) at {:?}", app_bytes.len(), path);
        }

        // create a storage directory
        let storage_path = self.karl_path.join("storage").join(&client.name);
        fs::create_dir_all(storage_path).unwrap();

        // register the client itself
        let mut clients = self.clients.lock().unwrap();
        let token = ClientToken::gen();
        if clients.insert(token.clone(), client.clone()).is_none() {
            info!("registered client {:?}", client);
        } else {
            unreachable!("impossible to generate duplicate client tokens")
        }
        let mut res = protos::RegisterResult::default();
        res.set_client_token(token.0);
        res
    }

    /// Notify the controller that a service is starting a request.
    ///
    /// Finds the host with the given service name and sets the active
    /// request to the given description. Logs an error message if the host
    /// cannot be found, or an already active request is overwritten.
    fn notify_start(&mut self, service_name: String, description: String) {
        info!("notify start name={:?} description={:?}", service_name, description);
        let mut unique_hosts = self.unique_hosts.lock().unwrap();
        if let Some(host) = unique_hosts.get_mut(&service_name) {
            if let Some(req) = &host.active_request {
                error!("overriding active request: {:?}", req)
            }
            host.last_msg = Instant::now();
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
    ///
    /// Also updates the request token.
    fn notify_end(&mut self, service_name: String, token: RequestToken) {
        info!("notify end name={:?} token={:?}", service_name, token);
        let mut unique_hosts = self.unique_hosts.lock().unwrap();
        if let Some(host) = unique_hosts.get_mut(&service_name) {
            if host.token.is_some() {
                error!("either the host sent a heartbeat during an \
                    active request or the host handled a request \
                    without us actually allocating the host. careful!")
            }
            host.token = Some(token);
            if let Some(mut req) = host.active_request.take() {
                req.end = Some(Instant::now());
                host.last_msg = Instant::now();
                host.last_request = Some(req);
                host.total += 1;
            } else {
                error!("no active request, null notify end");
            }
        } else {
            error!("missing host");
        }
    }

    /// Handle a host heartbeat, updating the request token for the
    /// host with the given service name.
    ///
    /// Parameters:
    /// - service_name - Service name.
    /// - token - New request token, or empty string if not renewed.
    fn heartbeat(&mut self, service_name: String, token: &str) {
        debug!("heartbeat {} {:?}", service_name, token);
        let mut unique_hosts = self.unique_hosts.lock().unwrap();
        if let Some(host) = unique_hosts.get_mut(&service_name) {
            host.last_msg = Instant::now();
            if !token.is_empty() {
                host.token = Some(Token(token.to_string()));
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

    const PASSWORD: &str = "password";

    /// Create a temporary directory for the karl path and initialize the
    /// controller with default password "password".
    fn init_test() -> (TempDir, Controller) {
        let dir = TempDir::new("karl").unwrap();
        let controller = Controller::new(dir.path().to_path_buf(), PASSWORD, false);
        (dir, controller)
    }

    /// Add a host named "host<i>" with socket addr "0.0.0.0:808<i>".
    fn add_host_test(c: &mut Controller, i: usize) {
        let name = format!("host{}", i);
        let addr: SocketAddr = format!("0.0.0.0:808{}", i).parse().unwrap();
        assert!(c.add_host(&name, addr, true, PASSWORD));
    }

    /// Wrapper around find_host for testing without having to clone locks.
    fn find_host_test(
        c: &mut Controller,
        token: &ClientToken,
        blocking: bool,
    ) -> protos::HostResult {
        Controller::find_host(
            c.hosts.clone(),
            c.unique_hosts.clone(),
            c.clients.clone(),
            token,
            blocking,
        )
    }

    #[test]
    fn test_add_host() {
        let (_karl_path, mut c) = init_test();
        assert!(c.hosts.lock().unwrap().0.is_empty());
        assert!(c.unique_hosts.lock().unwrap().is_empty());

        // Add a host
        let name = "host1";
        let addr: SocketAddr = "127.0.0.1:8081".parse().unwrap();
        assert!(c.add_host(name, addr, false, "password"));

        // Check controller was modified correctly
        let hosts = c.hosts.lock().unwrap();
        let unique_hosts = c.unique_hosts.lock().unwrap();
        assert_eq!(hosts.0.len(), 1);
        assert_eq!(unique_hosts.len(), 1);
        assert_eq!(hosts.0[0], name, "wrong service name");
        assert!(unique_hosts.get(name).is_some(), "violated host service name invariant");

        // Check host was initialized correctly
        let host = unique_hosts.get(name).unwrap();
        assert_eq!(host.index, 0);
        assert_eq!(host.name, name);
        assert_eq!(host.addr, addr);
        assert!(host.active_request.is_none());
        assert!(host.last_request.is_none());
        assert!(host.token.is_none());
        assert!(!host.confirmed);
    }

    #[test]
    fn test_add_host_password() {
        let (_karl_path, mut c) = init_test();
        let addr: SocketAddr = "127.0.0.1:8081".parse().unwrap();
        assert!(!c.add_host("host1", addr.clone(), false, "???"));
        assert!(c.add_host("host1", addr.clone(), false, "password"));
    }

    #[test]
    fn test_add_multiple_hosts() {
        let (_karl_path, mut c) = init_test();

        // Add three hosts
        add_host_test(&mut c, 1);
        add_host_test(&mut c, 2);
        add_host_test(&mut c, 3);

        // Check the index in unique_hosts corresponds to the index in hosts
        let hosts = c.hosts.lock().unwrap();
        let unique_hosts = c.unique_hosts.lock().unwrap();
        for (name, host) in unique_hosts.iter() {
            assert_eq!(name, &host.name);
            assert_eq!(hosts.0[host.index], host.name);
        }
    }

    #[test]
    fn test_remove_host() {
        let (_karl_path, mut c) = init_test();

        // Add hosts
        add_host_test(&mut c, 1);
        add_host_test(&mut c, 2);
        add_host_test(&mut c, 3);
        add_host_test(&mut c, 4);

        // Remove the last host
        let mut hosts = c.hosts.lock().unwrap();
        let mut unique_hosts = c.unique_hosts.lock().unwrap();
        assert!(hosts.0.contains(&"host4".to_string()));
        assert!(remove_host("host4", &mut hosts.0, &mut unique_hosts));
        assert!(!hosts.0.contains(&"host4".to_string()));
        assert_eq!(hosts.0.len(), 3);
        assert_eq!(unique_hosts.len(), 3);
        for host in unique_hosts.values() {
            assert_eq!(hosts.0[host.index], host.name);
        }

        // Remove the middle host
        assert!(hosts.0.contains(&"host2".to_string()));
        assert!(remove_host("host2", &mut hosts.0, &mut unique_hosts));
        assert!(!hosts.0.contains(&"host2".to_string()));
        assert_eq!(hosts.0.len(), 2);
        assert_eq!(unique_hosts.len(), 2);
        for host in unique_hosts.values() {
            assert_eq!(hosts.0[host.index], host.name);
        }

        // Remove the first host
        assert!(hosts.0.contains(&"host1".to_string()));
        assert!(remove_host("host1", &mut hosts.0, &mut unique_hosts));
        assert!(!hosts.0.contains(&"host1".to_string()));
        assert_eq!(hosts.0.len(), 1);
        assert_eq!(unique_hosts.len(), 1);
        for host in unique_hosts.values() {
            assert_eq!(hosts.0[host.index], host.name);
        }
    }

    #[test]
    fn test_add_remove_host_return_value() {
        let (_karl_path, mut c) = init_test();

        let addr: SocketAddr = "0.0.0.0:8081".parse().unwrap();
        assert!(c.add_host("host1", addr.clone(), true, "password"));
        assert!(!c.add_host("host1", addr.clone(), true, "password"));
        assert!(c.remove_host("host1"));
        assert!(!c.remove_host("host1"));
    }

    #[test]
    fn test_find_unconfirmed_host() {
        let (_karl_path, mut c) = init_test();

        // Register a client
        let client = c.register_client("name".to_string(), "0.0.0.0".parse().unwrap(), &vec![], true);
        let client_token = Token(client.get_client_token().to_string());
        let request_token = "requesttoken";

        // Add an unconfirmed host
        assert!(add_host(
            "host1",
            "0.0.0.0:8081".parse().unwrap(),
            &mut c.hosts.lock().unwrap().0,
            &mut c.unique_hosts.lock().unwrap(),
            false,
        ));
        assert!(!c.unique_hosts.lock().unwrap().get("host1").unwrap().confirmed);
        c.heartbeat("host1".to_string(), request_token);
        assert!(!find_host_test(&mut c, &client_token, false).get_found());

        // Confirm the host, and we should be able to discover it
        c.unique_hosts.lock().unwrap().get_mut("host1").unwrap().confirmed = true;
        assert!(find_host_test(&mut c, &client_token, false).get_found());
    }

    #[test]
    fn test_find_host_non_blocking() {
        let (_karl_path, mut c) = init_test();

        // Register a client
        let client = c.register_client("name".to_string(), "0.0.0.0".parse().unwrap(), &vec![], true);
        let client_token = Token(client.get_client_token().to_string());
        let request_token = "requesttoken";

        // Add three hosts
        add_host_test(&mut c, 1);
        add_host_test(&mut c, 2);
        add_host_test(&mut c, 3);
        assert_eq!(c.hosts.lock().unwrap().0.clone(), vec![
            "host1".to_string(),
            "host2".to_string(),
            "host3".to_string(),
        ]);
        c.heartbeat("host1".to_string(), request_token);
        c.heartbeat("host2".to_string(), request_token);
        c.heartbeat("host3".to_string(), request_token);

        // Set last_request of a host, say host 2.
        // find_host returns 2 3 1 round-robin.
        c.unique_hosts.lock().unwrap().get_mut("host2").unwrap().last_request = Some(Request::default());
        let host = find_host_test(&mut c, &client_token, false);
        assert!(host.get_found());
        assert_eq!(host.get_port(), 8082);
        let host = find_host_test(&mut c, &client_token, false);
        assert!(host.get_found());
        assert_eq!(host.get_port(), 8083);
        let host = find_host_test(&mut c, &client_token, false);
        assert!(host.get_found());
        assert_eq!(host.get_port(), 8081);
        c.heartbeat("host1".to_string(), request_token);
        c.heartbeat("host2".to_string(), request_token);
        c.heartbeat("host3".to_string(), request_token);

        // Make host 3 busy. (Reset request tokens)
        // find_host should return 2 1 2 round-robin.
        c.unique_hosts.lock().unwrap().get_mut("host3").unwrap().active_request = Some(Request::default());
        let host = find_host_test(&mut c, &client_token, false);
        assert!(host.get_found());
        assert_eq!(host.get_port(), 8082);
        c.heartbeat("host2".to_string(), request_token);
        let host = find_host_test(&mut c, &client_token, false);
        assert!(host.get_found());
        assert_eq!(host.get_port(), 8081);
        c.heartbeat("host1".to_string(), request_token);
        let host = find_host_test(&mut c, &client_token, false);
        assert!(host.get_found());
        assert_eq!(host.get_port(), 8082);
        c.heartbeat("host2".to_string(), request_token);

        // Make host 1 and 2 busy.
        // find_host should fail.
        c.unique_hosts.lock().unwrap().get_mut("host1").unwrap().active_request = Some(Request::default());
        c.unique_hosts.lock().unwrap().get_mut("host2").unwrap().active_request = Some(Request::default());
        let host = find_host_test(&mut c, &client_token, false);
        assert!(!host.get_found());
    }

    /// If the test times out, it actually succeeds!
    #[test]
    #[ignore]
    #[timeout(500)]
    fn test_find_host_blocking_no_hosts() {
        let (_karl_path, mut c) = init_test();
        // Register a client
        let client = c.register_client("name".to_string(), "0.0.0.0".parse().unwrap(), &vec![], true);
        let token = Token(client.get_client_token().to_string());
        let blocking = true;
        find_host_test(&mut c, &token, blocking);
        unreachable!("default controller should not return without hosts");
    }

    /// If the test times out, it actually succeeds!
    #[test]
    #[ignore]
    #[timeout(500)]
    fn test_find_host_blocking_unavailable() {
        let (_karl_path, mut c) = init_test();

        // Register a client
        let client = c.register_client("name".to_string(), "0.0.0.0".parse().unwrap(), &vec![], true);
        let client_token = Token(client.get_client_token().to_string());
        let request_token = "requesttoken";

        // Add a host.
        add_host_test(&mut c, 1);
        c.heartbeat("host1".to_string(), request_token);
        assert!(find_host_test(&mut c, &client_token, true).get_found());
        c.heartbeat("host1".to_string(), request_token);
        assert!(find_host_test(&mut c, &client_token, true).get_found());
        c.heartbeat("host1".to_string(), request_token);

        // Now make it busy.
        c.unique_hosts.lock().unwrap().get_mut("host1").unwrap().active_request = Some(Request::default());
        find_host_test(&mut c, &client_token, true);
        unreachable!("default controller should not return with busy hosts");
    }

    #[test]
    fn test_find_host_invalid_client_token() {
        let (_karl_path, mut c) = init_test();

        // Register a client
        let client = c.register_client("name".to_string(), "0.0.0.0".parse().unwrap(), &vec![], true);
        let token = Token(client.get_client_token().to_string());
        let bad_token1 = Token("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789ab".to_string());
        let bad_token2 = Token("badtoken".to_string());
        let request_token = "requesttoken";

        // Add a host.
        add_host_test(&mut c, 1);
        c.heartbeat("host1".to_string(), request_token);
        assert!(find_host_test(&mut c, &token, false).get_found());
        c.heartbeat("host1".to_string(), request_token);
        assert!(find_host_test(&mut c, &token, false).get_found());
        c.heartbeat("host1".to_string(), request_token);
        assert!(!find_host_test(&mut c, &bad_token1, false).get_found());
        assert!(!find_host_test(&mut c, &bad_token2, false).get_found());
        assert!(find_host_test(&mut c, &token, false).get_found());
    }

    #[test]
    fn test_find_host_unconfirmed_client_token() {
        let (_karl_path, mut c) = init_test();

        // Add a host.
        let request_token = "requesttoken";
        add_host_test(&mut c, 1);
        c.heartbeat("host1".to_string(), request_token);

        // Register an unconfirmed client
        let client = c.register_client("name".to_string(), "0.0.0.0".parse().unwrap(), &vec![], false);
        let token = Token(client.get_client_token().to_string());
        assert_eq!(c.clients.lock().unwrap().len(), 1);
        assert!(!c.clients.lock().unwrap().get(&token).unwrap().confirmed);
        assert!(!find_host_test(&mut c, &token, false).get_found(),
            "found host with unconfirmed token");

        // Confirm the client and find a host.
        c.clients.lock().unwrap().get_mut(&token).unwrap().confirmed = true;
        assert!(find_host_test(&mut c, &token, false).get_found(),
            "failed to find host with confirmed token");
    }

    #[test]
    fn test_find_host_request_tokens() {
        let (_karl_path, mut c) = init_test();

        // Register a client
        let client = c.register_client("name".to_string(), "0.0.0.0".parse().unwrap(), &vec![], true);
        let token = Token(client.get_client_token().to_string());
        let request_token = "requesttoken";

        // Add a host.
        add_host_test(&mut c, 1);
        add_host_test(&mut c, 2);
        assert!(!find_host_test(&mut c, &token, false).get_found(), "no request tokens");
        c.heartbeat("host1".to_string(), request_token);
        assert!(find_host_test(&mut c, &token, false).get_found(), "set token in heartbeat");
        assert!(!find_host_test(&mut c, &token, false).get_found(), "reset token");
        c.heartbeat("host1".to_string(), request_token);
        c.heartbeat("host2".to_string(), request_token);
        assert!(find_host_test(&mut c, &token, false).get_found());
        assert!(find_host_test(&mut c, &token, false).get_found());
        assert!(!find_host_test(&mut c, &token, false).get_found());
    }

    #[test]
    fn test_register_client() {
        let (karl_path, mut c) = init_test();
        let storage_path = karl_path.path().join("storage").join("hello");
        let app_path = karl_path.path().join("www").join("hello.hbs");

        // Check initial conditions.
        assert!(c.clients.lock().unwrap().is_empty());
        assert!(!storage_path.exists());
        assert!(!app_path.exists());

        // Register a client with an app.
        let name = "hello";
        let client_ip: IpAddr = "127.0.0.1".parse().unwrap();
        let app = vec![10, 10, 10, 10];
        c.register_client(name.to_string(), client_ip, &app, true);
        assert_eq!(c.clients.lock().unwrap().len(), 1, "registered client");

        // Check the client was registered correcftly.
        let client = c.clients.lock().unwrap().values().next().unwrap().clone();
        assert_eq!(client.name, name);
        assert_eq!(client.addr, client_ip);
        assert!(storage_path.is_dir(), "storage dir was not created");
        assert!(app_path.is_file(), "app was not created and renamed");

        // Register a client without an app.
        // Storage directory should be created, but app file should not.
        let app: Vec<u8> = vec![];
        c.register_client("world".to_string(), client_ip, &app, true);
        assert_eq!(c.clients.lock().unwrap().len(), 2);
        assert!(karl_path.path().join("storage").join("world").is_dir());
        assert!(!karl_path.path().join("www").join("world.hbs").exists());
    }

    #[test]
    fn test_register_duplicate_clients() {
        let (karl_path, mut c) = init_test();
        let storage_base = karl_path.path().join("storage");
        let app_base = karl_path.path().join("www");

        // Register a client with an app.
        let name = "hello";
        let client_ip: IpAddr = "127.0.0.1".parse().unwrap();
        let app = vec![10, 10, 10, 10];
        c.register_client(name.to_string(), client_ip.clone(), &app, true);
        c.register_client(name.to_string(), client_ip.clone(), &app, true);
        c.register_client(name.to_string(), client_ip.clone(), &app, true);

        // Check the clients have different names.
        let names = c.clients.lock().unwrap().values().map(|client| client.name.clone()).collect::<Vec<_>>();
        assert_eq!(names.len(), 3);
        assert!(names.contains(&"hello".to_string()));
        assert!(names.contains(&"hello_1".to_string()));
        assert!(names.contains(&"hello_2".to_string()));

        // Check the clients all have registered storage and app paths.
        assert!(storage_base.join("hello").is_dir());
        assert!(storage_base.join("hello_1").is_dir());
        assert!(storage_base.join("hello_2").is_dir());
        assert!(app_base.join("hello.hbs").is_file());
        assert!(app_base.join("hello_1.hbs").is_file());
        assert!(app_base.join("hello_2.hbs").is_file());
    }

    #[test]
    fn test_register_format_names() {
        let (_karl_path, mut c) = init_test();

        let client_ip: IpAddr = "127.0.0.1".parse().unwrap();
        c.register_client("   leadingwhitespace".to_string(), client_ip.clone(), &vec![], true);
        c.register_client("trailingwhitespace \t".to_string(), client_ip.clone(), &vec![], true);
        c.register_client("uPpErCaSe".to_string(), client_ip.clone(), &vec![], true);
        c.register_client("iNv@l1d_ch@r$".to_string(), client_ip.clone(), &vec![], true);
        c.register_client("\tEVERYth!ng   \n\r".to_string(), client_ip.clone(), &vec![], true);

        // Check the names were formatted correctly (trimmed and lowercase).
        let names = c.clients.lock().unwrap().values().map(|client| client.name.clone()).collect::<Vec<_>>();
        assert_eq!(names.len(), 5);
        assert!(names.contains(&"leadingwhitespace".to_string()));
        assert!(names.contains(&"trailingwhitespace".to_string()));
        assert!(names.contains(&"uppercase".to_string()));
        assert!(names.contains(&"invl1d_chr".to_string()));
        assert!(names.contains(&"everythng".to_string()));
    }

    #[test]
    fn test_register_client_result() {
        let (_karl_path, mut c) = init_test();
        let res1 = c.register_client("c1".to_string(), "1.0.0.0".parse().unwrap(), &vec![], true);
        let res2 = c.register_client("c2".to_string(), "2.0.0.0".parse().unwrap(), &vec![], true);
        let res3 = c.register_client("c3".to_string(), "3.0.0.0".parse().unwrap(), &vec![], true);

        // Tokens are unique
        assert!(res1.client_token != res2.client_token);
        assert!(res1.client_token != res3.client_token);
        assert!(res2.client_token != res3.client_token);
    }

    #[test]
    fn test_notify_start_no_hosts() {
        let (_karl_path, mut c) = init_test();

        // Notify start with no hosts. Nothing errors.
        let service_name = "host1".to_string();
        let description = "my first app :)";
        c.notify_start(service_name.clone(), description.to_string());

        // Create a host and notify start.
        add_host_test(&mut c, 1);
        assert!(c.unique_hosts.lock().unwrap().get("host1").unwrap().active_request.is_none(),
            "no initial active request");
        c.notify_start(service_name.clone(), description.to_string());
        let request = c.unique_hosts.lock().unwrap().get("host1").unwrap().active_request.clone();
        assert!(request.is_some(), "active request started");
        let request = request.unwrap();
        assert_eq!(request.description, description, "same description");
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
        let (_karl_path, mut c) = init_test();

        // Notify end with no hosts. Nothing errors.
        let service_name = "host1".to_string();
        let description = "description".to_string();
        let token = Token("abc123".to_string());
        c.notify_end(service_name.clone(), token.clone());

        // Create a host. Notify end does not do anything without an active request.
        add_host_test(&mut c, 1);
        assert!(c.unique_hosts.lock().unwrap().get("host1").unwrap().active_request.is_none());
        assert!(c.unique_hosts.lock().unwrap().get("host1").unwrap().last_request.is_none());
        c.notify_end(service_name.clone(), token.clone());
        assert!(c.unique_hosts.lock().unwrap().get("host1").unwrap().active_request.is_none());
        assert!(c.unique_hosts.lock().unwrap().get("host1").unwrap().last_request.is_none());

        // Notify start then notify end.
        c.notify_start(service_name.clone(), description.clone());
        thread::sleep(Duration::from_secs(2));
        c.notify_end(service_name.clone(), token.clone());
        assert!(c.unique_hosts.lock().unwrap().get("host1").unwrap().active_request.is_none());
        assert!(c.unique_hosts.lock().unwrap().get("host1").unwrap().last_request.is_some());
        let request = c.unique_hosts.lock().unwrap().get("host1").unwrap().last_request.clone().unwrap();
        assert!(request.description == description);
        assert!(request.end.is_some());
        assert!(request.end.unwrap() > request.start);
    }

    #[test]
    fn host_heartbeat_sets_request_token() {
        let (_karl_path, mut c) = init_test();
        let name = "host1".to_string();
        let token1 = "abc";
        let token2 = "def";
        add_host_test(&mut c, 1);
        assert!(
            c.unique_hosts.lock().unwrap().get(&name).unwrap().token.is_none(),
            "no token initially",
        );
        c.heartbeat(name.clone(), "");
        assert!(
            c.unique_hosts.lock().unwrap().get(&name).unwrap().token.is_none(),
            "empty token doesn't change the token",
        );
        c.heartbeat(name.clone(), token1);
        assert_eq!(
            c.unique_hosts.lock().unwrap().get(&name).unwrap().token,
            Some(Token(token1.to_string())),
            "heartbeat sets the initial token",
        );
        c.heartbeat(name.clone(), "");
        assert_eq!(
            c.unique_hosts.lock().unwrap().get(&name).unwrap().token,
            Some(Token(token1.to_string())),
            "empty token doesn't change the token",
        );
        c.heartbeat(name.clone(), token2);
        assert_eq!(
            c.unique_hosts.lock().unwrap().get(&name).unwrap().token,
            Some(Token(token2.to_string())),
            "heartbeat can also replace tokens",
        );
    }

    #[test]
    fn host_messages_update_last_msg_time() {
        let (_karl_path, mut c) = init_test();
        let name = "host1".to_string();
        add_host_test(&mut c, 1);

        let t1 = c.unique_hosts.lock().unwrap().get(&name).unwrap().last_msg.clone();
        thread::sleep(Duration::from_secs(1));
        c.heartbeat(name.clone(), "token");
        let t2 = c.unique_hosts.lock().unwrap().get(&name).unwrap().last_msg.clone();
        thread::sleep(Duration::from_secs(1));
        c.heartbeat(name.clone(), "");
        let t3 = c.unique_hosts.lock().unwrap().get(&name).unwrap().last_msg.clone();
        thread::sleep(Duration::from_secs(1));
        c.notify_start(name.clone(), "description".to_string());
        let t4 = c.unique_hosts.lock().unwrap().get(&name).unwrap().last_msg.clone();
        thread::sleep(Duration::from_secs(1));
        c.notify_end(name.clone(), Token("token".to_string()));
        let t5 = c.unique_hosts.lock().unwrap().get(&name).unwrap().last_msg.clone();
        assert!(t2 > t1, "regular heartbeat updates time");
        assert!(t3 > t2, "empty heartbeat also updates time");
        assert!(t4 > t3, "notify start updates time");
        assert!(t5 > t4, "notify end updates time");
    }

    #[test]
    fn test_notify_end_also_resets_request_tokens() {
        let (_karl_path, mut c) = init_test();

        let host1 = "host1".to_string();
        let client = c.register_client("name".to_string(), "0.0.0.0".parse().unwrap(), &vec![], true);
        let client_token = Token(client.get_client_token().to_string());
        let request_token1 = Token("requesttoken1".to_string());
        let request_token2 = Token("requesttoken2".to_string());
        add_host_test(&mut c, 1);

        // Heartbeat
        assert!(c.unique_hosts.lock().unwrap().get(&host1).unwrap().token.is_none());
        c.heartbeat(host1.clone(), &request_token1.0);
        assert!(c.unique_hosts.lock().unwrap().get(&host1).unwrap().token.is_some());

        // HostRequest
        let host = find_host_test(&mut c, &client_token, false);
        assert!(c.unique_hosts.lock().unwrap().get(&host1).unwrap().token.is_none());
        assert!(host.get_found());
        assert_eq!(host.get_request_token(), request_token1.0);

        // NotifyStart
        c.notify_start(host1.clone(), "description".to_string());
        assert!(c.unique_hosts.lock().unwrap().get(&host1).unwrap().token.is_none());

        // NotifyEnd
        c.notify_end(host1.clone(), request_token2.clone());
        assert!(c.unique_hosts.lock().unwrap().get(&host1).unwrap().token.is_some());

        // HostRequest
        let host = find_host_test(&mut c, &client_token, false);
        assert!(c.unique_hosts.lock().unwrap().get(&host1).unwrap().token.is_none());
        assert!(host.get_found());
        assert_eq!(host.get_request_token(), request_token2.0);
    }

    #[test]
    fn test_notify_end_updates_total_number_of_requests() {
        let (_karl_path, mut c) = init_test();

        let host1 = "host1".to_string();
        let client = c.register_client("name".to_string(), "0.0.0.0".parse().unwrap(), &vec![], true);
        let client_token = Token(client.get_client_token().to_string());
        let request_token1 = Token("requesttoken1".to_string());
        let request_token2 = Token("requesttoken2".to_string());
        add_host_test(&mut c, 1);

        // No requests initially.
        assert_eq!(c.unique_hosts.lock().unwrap().get(&host1).unwrap().total, 0);
        c.heartbeat(host1.clone(), &request_token1.0);
        assert_eq!(c.unique_hosts.lock().unwrap().get(&host1).unwrap().total, 0);
        let host = find_host_test(&mut c, &client_token, false);
        assert!(host.get_found());
        assert_eq!(host.get_request_token(), request_token1.0);
        assert_eq!(c.unique_hosts.lock().unwrap().get(&host1).unwrap().total, 0);
        c.notify_start(host1.clone(), "description".to_string());
        assert_eq!(c.unique_hosts.lock().unwrap().get(&host1).unwrap().total, 0);

        // One request.
        c.notify_end(host1.clone(), request_token2.clone());
        assert_eq!(c.unique_hosts.lock().unwrap().get(&host1).unwrap().total, 1);
        let host = find_host_test(&mut c, &client_token, false);
        assert!(host.get_found());
        assert_eq!(host.get_request_token(), request_token2.0);
        assert_eq!(c.unique_hosts.lock().unwrap().get(&host1).unwrap().total, 1);
        c.notify_start(host1.clone(), "description".to_string());
        assert_eq!(c.unique_hosts.lock().unwrap().get(&host1).unwrap().total, 1);

        // Two requests.
        c.notify_end(host1.clone(), request_token1.clone());
        assert_eq!(c.unique_hosts.lock().unwrap().get(&host1).unwrap().total, 2);
    }

    #[test]
    fn test_verify_host() {
        let (_karl_path, mut c) = init_test();
        let addr: SocketAddr = "1.2.3.4:8080".parse().unwrap();
        let ip: IpAddr = addr.ip();
        assert!(c.add_host("host1", addr, true, "password"));

        assert!(c.verify_host_name("host2", &ip).is_err(), "invalid host name");
        assert!(c.verify_host_name("host1", &ip).is_ok(), "valid name and ip");
        let localhost1: IpAddr = "127.0.0.1".parse().unwrap();
        let localhost2: IpAddr = "0.0.0.0".parse().unwrap();
        assert!(c.verify_host_name("host1", &localhost1).is_ok(), "localhost also ok");
        assert!(c.verify_host_name("host1", &localhost2).is_ok(), "localhost also ok");
    }

    #[test]
    fn test_autoconfirm_client() {
        let dir1 = TempDir::new("karl").unwrap();
        let dir2 = TempDir::new("karl").unwrap();
        let mut c1 = Controller::new(dir1.path().to_path_buf(), PASSWORD, false);
        let mut c2 = Controller::new(dir2.path().to_path_buf(), PASSWORD, true);

        let name: String = "client_name".to_string();
        let addr: IpAddr = "1.2.3.4".parse().unwrap();
        c1.register_client(name.clone(), addr.clone(), &vec![], false);
        c2.register_client(name.clone(), addr.clone(), &vec![], false);
        let client1 = c1.clients.lock().unwrap().values().next().unwrap().confirmed;
        let client2 = c2.clients.lock().unwrap().values().next().unwrap().confirmed;
        assert!(!client1);
        assert!(client2);
    }

    #[test]
    fn test_autoconfirm_host() {
        let dir1 = TempDir::new("karl").unwrap();
        let dir2 = TempDir::new("karl").unwrap();
        let mut c1 = Controller::new(dir1.path().to_path_buf(), PASSWORD, false);
        let mut c2 = Controller::new(dir2.path().to_path_buf(), PASSWORD, true);

        let addr: SocketAddr = "1.2.3.4:8080".parse().unwrap();
        assert!(c1.add_host("host", addr.clone(), false, PASSWORD));
        assert!(c2.add_host("host", addr.clone(), false, PASSWORD));
        let host1 = c1.unique_hosts.lock().unwrap().values().next().unwrap().confirmed;
        let host2 = c2.unique_hosts.lock().unwrap().values().next().unwrap().confirmed;
        assert!(!host1);
        assert!(host2);
    }
}
