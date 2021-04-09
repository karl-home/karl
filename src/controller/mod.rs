pub mod types;
mod scheduler;
mod data;
mod audit;
mod runner;
pub use scheduler::HostScheduler;
pub use data::DataSink;
pub use audit::{AuditLog, LogEntry, LogEntryType};
pub use runner::HookRunner;
use types::*;

use std::collections::{HashSet, HashMap};
use std::path::PathBuf;
use std::net::{TcpStream, TcpListener, IpAddr};
use std::sync::{Arc, Mutex};
use std::time::{Instant, Duration};
use std::thread;
use std::fs;
use std::io::Read;

use tokio::runtime::Runtime;
use protobuf::{Message, parse_from_bytes, ProtobufEnum};
use crate::dashboard;
use crate::packet;
use crate::protos::{self, MessageType};
use crate::common::{Error, Token, ClientToken};

/// Controller used for discovering available Karl services and coordinating
/// client requests among available services.
pub struct Controller {
    pub rt: Runtime,
    karl_path: PathBuf,
    /// Data structure for adding and allocating hosts
    scheduler: Arc<Mutex<HostScheduler>>,
    /// Data structure for managing sensor data.
    data_sink: Arc<Mutex<DataSink>>,
    /// Data structure for queueing and spawning processes from hooks.
    runner: HookRunner,
    /// Audit log indexed by process ID and path accessed.
    audit_log: Arc<Mutex<AuditLog>>,
    /// Map from client token to client.
    ///
    /// Unique identifier for the client, known only by the controller
    /// and the client itself. Generated on registration. All host
    /// requests from the client to the controller must include this token.
    clients: Arc<Mutex<HashMap<ClientToken, Client>>>,
    /// Whether to automatically confirm clients and hosts.
    autoconfirm: bool,
}

impl Controller {
    /// Create a new controller.
    ///
    /// Call `start()` after constructing the controller to ensure it is
    /// listening for hosts and client requests.
    pub fn new(karl_path: PathBuf, password: &str, autoconfirm: bool) -> Self {
        let scheduler = Arc::new(Mutex::new(HostScheduler::new(password)));
        Controller {
            rt: Runtime::new().unwrap(),
            karl_path: karl_path.clone(),
            scheduler: scheduler.clone(),
            data_sink: Arc::new(Mutex::new(DataSink::new(karl_path))),
            runner: HookRunner::new(scheduler),
            audit_log: Arc::new(Mutex::new(AuditLog::new())),
            clients: Arc::new(Mutex::new(HashMap::new())),
            autoconfirm,
        }
    }

    /// Initializes clients based on the `<KARL_PATH>/clients.txt` file and
    /// starts the web dashboard. Also starts a TCP listener for the following
    /// messages:
    /// - HostRequest
    /// - RegisterRequest
    /// - NotifyStart
    /// - NotifyEnd
    /// - HostHeartbeat
    /// - HostRegisterRequest
    /// - PingRequest
    pub fn start(
        &mut self,
        port: u16,
    ) -> Result<(), Error> {
        // Make the karl path if it doesn't already exist.
        fs::create_dir_all(&self.karl_path).unwrap();

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
            self.scheduler.clone(),
            self.clients.clone(),
        );
        // Start the hook runner.
        self.runner.start(false);
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
        let ty = MessageType::from_i32(
            req_header.ty).ok_or(Error::InvalidPacketType(req_header.ty))?;
        match ty {
            MessageType::HOST_REQUEST => {
                let req = parse_from_bytes::<protos::HostRequest>(&req_bytes)
                    .map_err(|e| Error::SerializationError(format!("{:?}", e)))
                    .unwrap();
                let scheduler = self.scheduler.clone();
                let clients = self.clients.clone();
                if req.blocking {
                    self.rt.spawn(async move {
                        let res = Self::find_host(
                            scheduler,
                            clients,
                            &Token(req.client_token),
                            req.blocking,
                        );
                        let res_bytes = res.write_to_bytes().unwrap();

                        // Return the result to sender.
                        trace!("writing packet ({} bytes) {:?}", res_bytes.len(), res_bytes);
                        let now = Instant::now();
                        packet::write(&mut stream, MessageType::HOST_RESULT, &res_bytes).unwrap();
                        trace!("=> {} s", now.elapsed().as_secs_f32());
                    });
                } else {
                    let res = Self::find_host(
                        scheduler,
                        clients,
                        &Token(req.client_token),
                        req.blocking,
                    );
                    let res_bytes = res.write_to_bytes().unwrap();

                    // Return the result to sender.
                    trace!("writing packet ({} bytes) {:?}", res_bytes.len(), res_bytes);
                    let now = Instant::now();
                    packet::write(&mut stream, MessageType::HOST_RESULT, &res_bytes)?;
                    trace!("=> {} s", now.elapsed().as_secs_f32());
                }
            },
            MessageType::REGISTER_REQUEST => {
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
                packet::write(&mut stream, MessageType::REGISTER_RESULT, &res_bytes)?;
                debug!("=> {} s", now.elapsed().as_secs_f32());
            },
            MessageType::NOTIFY_START => {
                let req = parse_from_bytes::<protos::NotifyStart>(&req_bytes)
                    .map_err(|e| Error::SerializationError(format!("{:?}", e)))
                    .unwrap();
                let ip = stream.peer_addr().unwrap().ip();
                let mut scheduler = self.scheduler.lock().unwrap();
                scheduler.verify_host_name(&req.service_name, &ip)?;
                scheduler.notify_start(req.service_name, req.description);
            },
            MessageType::NOTIFY_END => {
                let req = parse_from_bytes::<protos::NotifyEnd>(&req_bytes)
                    .map_err(|e| Error::SerializationError(format!("{:?}", e)))
                    .unwrap();
                let ip = stream.peer_addr().unwrap().ip();
                let mut scheduler = self.scheduler.lock().unwrap();
                scheduler.verify_host_name(&req.service_name, &ip)?;
                scheduler.notify_end(req.service_name, Token(req.request_token));
            },
            MessageType::HOST_HEARTBEAT => {
                let req = parse_from_bytes::<protos::HostHeartbeat>(&req_bytes)
                    .map_err(|e| Error::SerializationError(format!("{:?}", e)))
                    .unwrap();
                let ip = stream.peer_addr().unwrap().ip();
                let mut scheduler = self.scheduler.lock().unwrap();
                scheduler.verify_host_name(&req.service_name, &ip)?;
                scheduler.heartbeat(req.service_name, &req.request_token);
            },
            MessageType::HOST_REGISTER_REQUEST => {
                let req = parse_from_bytes::<protos::HostRegisterRequest>(&req_bytes)
                    .map_err(|e| Error::SerializationError(format!("{:?}", e)))
                    .unwrap();
                let ip = stream.peer_addr().unwrap().ip();
                let mut scheduler = self.scheduler.lock().unwrap();
                if !scheduler.add_host(
                    &req.service_name,
                    format!("{}:{}", req.ip, req.port).parse().unwrap(),
                    self.autoconfirm,
                    req.get_password(),
                ) {
                    warn!("failed to register {} ({:?})", &req.service_name, ip);
                }
                scheduler.verify_host_name(&req.service_name, &ip)?;
            },
            MessageType::PING_REQUEST => {
                parse_from_bytes::<protos::PingRequest>(&req_bytes)
                    .map_err(|e| Error::SerializationError(format!("{:?}", e)))
                    .unwrap();
                let res = protos::PingResult::default();
                let res_bytes = res.write_to_bytes().unwrap();

                // Return the result to sender.
                debug!("writing packet {:?} ({} bytes)", res_bytes, res_bytes.len());
                let now = Instant::now();
                packet::write(&mut stream, MessageType::PING_RESULT, &res_bytes)?;
                debug!("=> {} s", now.elapsed().as_secs_f32());
            },
            ty => return Err(Error::InvalidPacketType(ty.value())),
        };
        Ok(())
    }

    /// Find a host to connect to round-robin, authenticating the client.
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
        scheduler: Arc<Mutex<HostScheduler>>,
        clients: Arc<Mutex<HashMap<ClientToken, Client>>>,
        token: &ClientToken,
        blocking: bool,
    ) -> protos::HostResult {
        // Validate the client token.
        let res = protos::HostResult::default();
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
            if let Some(host) = scheduler.lock().unwrap().find_host() {
                return host;
            }
            if !blocking {
                println!("find_host no hosts available {:?}", token);
                return res;
            }
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
}

#[cfg(test)]
mod test {
    use super::*;
    use std::net::SocketAddr;
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
        assert!(c.scheduler.lock().unwrap().add_host(&name, addr, true, PASSWORD));
    }

    /// Wrapper around find_host for testing without having to clone locks.
    fn find_host_test(
        c: &mut Controller,
        token: &ClientToken,
        blocking: bool,
    ) -> protos::HostResult {
        Controller::find_host(
            c.scheduler.clone(),
            c.clients.clone(),
            token,
            blocking,
        )
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
        c.scheduler.lock().unwrap().heartbeat("host1".to_string(), request_token);
        assert!(find_host_test(&mut c, &client_token, true).get_found());
        c.scheduler.lock().unwrap().heartbeat("host1".to_string(), request_token);
        assert!(find_host_test(&mut c, &client_token, true).get_found());
        c.scheduler.lock().unwrap().heartbeat("host1".to_string(), request_token);

        // Now make it busy.
        c.scheduler.lock().unwrap().notify_start(
            "host1".to_string(), "description".to_string());
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
        c.scheduler.lock().unwrap().heartbeat("host1".to_string(), request_token);
        assert!(find_host_test(&mut c, &token, false).get_found());
        c.scheduler.lock().unwrap().heartbeat("host1".to_string(), request_token);
        assert!(find_host_test(&mut c, &token, false).get_found());
        c.scheduler.lock().unwrap().heartbeat("host1".to_string(), request_token);
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
        c.scheduler.lock().unwrap().heartbeat("host1".to_string(), request_token);

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
}
