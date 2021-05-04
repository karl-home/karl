mod scheduler;
mod data;
mod audit;
mod runner;
pub use scheduler::HostScheduler;
pub use data::DataSink;
pub use audit::{AuditLog, LogEntry, LogEntryType};
pub use runner::HookRunner;

use std::collections::{HashSet, HashMap};
use std::path::{Path, PathBuf};
use std::net::IpAddr;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;
use std::fs;

use tonic::{Request, Response, Status, Code};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use crate::protos::*;
use crate::dashboard;
use karl_common::*;

pub fn to_status(e: Error) -> Status {
    match e {
        Error::InvalidHostMessage(s) => Status::new(Code::Unauthenticated, s),
        e => Status::new(Code::Unknown, format!("{:?}", e)),
    }
}

/// Controller used for discovering available Karl services and coordinating
/// client requests among available services.
pub struct Controller {
    karl_path: PathBuf,
    /// Data structure for adding and allocating hosts
    scheduler: Arc<Mutex<HostScheduler>>,
    /// Data structure for managing sensor data.
    data_sink: Arc<RwLock<DataSink>>,
    /// Data structure for queueing and spawning processes from hooks.
    runner: HookRunner,
    /// Audit log indexed by process ID and path accessed.
    audit_log: Arc<Mutex<AuditLog>>,
    /// Map from client token to client.
    ///
    /// Unique identifier for the sensor, known only by the controller
    /// and the sensor itself. Generated on registration. All host
    /// requests from the sensor to the controller must include this token.
    sensors: Arc<Mutex<HashMap<SensorToken, Client>>>,
    /// Sender channels to forward state changes to the sender's
    /// streaming connection.
    state: Arc<RwLock<HashMap<SensorID, mpsc::Sender<StateChangePair>>>>,
    /// Whether to automatically confirm sensors and hosts.
    autoconfirm: bool,
}

#[tonic::async_trait]
impl karl_controller_server::KarlController for Controller {
    // hosts

    async fn host_register(
        &self, req: Request<HostRegisterRequest>,
    ) -> Result<Response<HostRegisterResult>, Status> {
        let now = Instant::now();
        let req = req.into_inner();
        let mut scheduler = self.scheduler.lock().unwrap();
        if let Some(host_token) = scheduler.add_host(
            req.host_id.clone(),
            format!("{}:{}", req.ip, req.port).parse().unwrap(),
            self.autoconfirm,
            &req.password,
        ) {
            trace!("host_register => {} s", now.elapsed().as_secs_f32());
            Ok(Response::new(HostRegisterResult { host_token }))
        } else {
            warn!("failed to register {} ({:?})", &req.host_id, req.ip);
            Err(Status::new(Code::InvalidArgument, "incorrect password or duplicate host ID"))
        }
    }

    async fn forward_network(
        &self, req: Request<NetworkAccess>,
    ) -> Result<Response<()>, Status> {
        // TODO: validate host token
        let req = req.into_inner();
        self.audit_log.lock().unwrap().push(
            &req.process_token,
            LogEntryType::Network { domain: req.domain },
        );
        Ok(Response::new(()))
    }

    async fn forward_get(
        &self, req: Request<GetData>,
    ) -> Result<Response<GetDataResult>, Status> {
        // TODO: validate host token
        let req = req.into_inner();
        // self.audit_log.lock().unwrap().push(
        //     &req.process_token,
        //     LogEntryType::Get { path: req.path },
        // );
        let res = self.data_sink.read().unwrap()
            .get_data(req.tag, req.lower, req.upper)
            .map_err(|e| to_status(e))?;
        Ok(Response::new(GetDataResult {
            data: res.data,
            timestamps: res.timestamps,
        }))
    }

    async fn forward_push(
        &self, req: Request<PushData>,
    ) -> Result<Response<()>, Status> {
        // TODO: validate host token
        warn!("FINISH step 8: push box");
        let req = req.into_inner();
        // self.audit_log.lock().unwrap().push(
        //     &req.process_token,
        //     LogEntryType::Put { path: req.path.clone() },
        // );
        let (tag, timestamp) = self.data_sink.write().unwrap()
            .push_data(req.tag, req.data)
            .map_err(|e| to_status(e))?;
        // TODO: move to its own thread
        self.runner.spawn_if_watched(tag, timestamp).await;
        Ok(Response::new(()))
    }

    async fn forward_state(
        &self, req: Request<StateChange>,
    ) -> Result<Response<()>, Status> {
        // TODO: validate host token
        let req = req.into_inner();
        let tx = if let Some(tx) = self.state.read().unwrap().get(&req.sensor_id) {
            tx.clone()
        } else {
            return Err(Status::new(Code::NotFound, "sensor not listening"));
        };
        self.audit_log.lock().unwrap().push(
            &req.process_token,
            LogEntryType::State {
                sensor_id: req.sensor_id,
                key: req.key.clone(),
                value: req.value.clone(),
            },
        );
        let pair = StateChangePair {
            key: req.key,
            value: req.value,
        };
        tx.send(pair).await.unwrap();
        Ok(Response::new(()))
    }

    async fn finish_compute(
        &self, req: Request<NotifyEnd>,
    ) -> Result<Response<()>, Status> {
        let now = Instant::now();
        let req = req.into_inner();
        self.scheduler.lock().unwrap().notify_end(req.host_token, req.process_token.clone())?;
        self.audit_log.lock().unwrap().notify_end(req.process_token);
        trace!("notify_end => {} s", now.elapsed().as_secs_f32());
        Ok(Response::new(()))
    }

    async fn heartbeat(
        &self, req: Request<HostHeartbeat>,
    ) -> Result<Response<()>, Status> {
        let now = Instant::now();
        let mut scheduler = self.scheduler.lock().unwrap();
        scheduler.heartbeat(req.into_inner().host_token);
        trace!("heartbeat => {} s", now.elapsed().as_secs_f32());
        Ok(Response::new(()))
    }

    // sensors

    async fn sensor_register(
        &self, req: Request<SensorRegisterRequest>,
    ) -> Result<Response<SensorRegisterResult>, Status> {
        let now = Instant::now();
        let req = req.into_inner();
        let res = self.sensor_register(
            req.global_sensor_id,
            "0.0.0.0".parse().unwrap(), // TODO?
            req.keys,
            req.tags.clone(),
            &req.app[..],
            false,
        );
        self.data_sink.write().unwrap()
            .new_entity(res.sensor_id.clone(), req.tags)
            .map_err(|e| to_status(e))?;
        trace!("sensor_register => {} s", now.elapsed().as_secs_f32());
        Ok(Response::new(res))
    }

    async fn push_raw_data(
        &self, req: Request<SensorPushData>,
    ) -> Result<Response<()>, Status> {
        let now = Instant::now();
        warn!("step 2: persisting data");
        let req = req.into_inner();
        let sensor_id = {
            let sensors = self.sensors.lock().unwrap();
            if let Some(sensor) = sensors.get(&req.sensor_token) {
                sensor.id.clone()
            } else {
                return Err(Status::new(Code::Unauthenticated, "invalid sensor token"));
            }
        };
        let (tag, timestamp) = self.data_sink.write().unwrap()
            .push_sensor_data(sensor_id, req.tag, req.data)
            .map_err(|e| to_status(e))?;
        warn!("=> {} s", now.elapsed().as_secs_f32());
        self.runner.spawn_if_watched(tag, timestamp).await;
        Ok(Response::new(()))
    }

    type StateChangesStream = ReceiverStream<Result<StateChangePair, Status>>;

    async fn state_changes(
        &self, req: Request<StateChangeInit>,
    ) -> Result<Response<Self::StateChangesStream>, Status> {
        let req = req.into_inner();
        let sensor_id = {
            if let Some(sensor) = self.sensors.lock().unwrap().get(&req.sensor_token) {
                sensor.id.clone()
            } else {
                return Err(Status::new(Code::Unauthenticated, "invalid token"));
            }
        };

        let (internal_tx, mut internal_rx) = mpsc::channel::<StateChangePair>(10);
        let (tx, rx) = mpsc::channel::<Result<StateChangePair, Status>>(10);
        self.state.write().unwrap().insert(sensor_id, internal_tx);
        tokio::spawn(async move {
            while let Some(pair) = internal_rx.recv().await {
                tx.send(Ok(pair)).await.unwrap();
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    // users

    async fn audit(
        &self, req: Request<AuditRequest>,
    ) -> Result<Response<AuditResult>, Status> {
        let req = req.into_inner();
        let entries = if !req.path.is_empty() {
            let path = sanitize_path(&req.path);
            self.audit_log.lock().unwrap().audit_file(Path::new(&path))
        } else if !req.sensor_id.is_empty() {
            self.audit_log.lock().unwrap().audit_sensor(&req.sensor_id)
        } else {
            self.audit_log.lock().unwrap().audit_process(req.pid)
        };
        let entries = entries.ok_or(Status::new(Code::NotFound, "not found"))?;
        Ok(Response::new(AuditResult { entries }))
    }

    async fn verify_sensor(
        &self, _req: Request<VerifySensorRequest>,
    ) -> Result<Response<()>, Status> {
        unimplemented!()
    }

    async fn verify_host(
        &self, _req: Request<VerifyHostRequest>,
    ) -> Result<Response<()>, Status> {
        unimplemented!()
    }

    async fn register_hook(
        &self, req: Request<RegisterHookRequest>,
    ) -> Result<Response<RegisterHookResult>, Status> {
        let now = Instant::now();
        let req = req.into_inner();
        let hook_id = self.register_hook(
            &req.token.to_string(),
            req.global_hook_id.to_string(),
        ).map_err(|e| to_status(e))?;
        trace!("register_hook => {} s", now.elapsed().as_secs_f32());
        Ok(Response::new(RegisterHookResult { hook_id }))
    }

    async fn add_data_edge(
        &self, req: Request<AddDataEdgeRequest>,
    ) -> Result<Response<()>, Status> {
        let req = req.into_inner();
        info!("add_data_edge {}.{} -> {} stateless={}",
            req.output_id, req.output_tag, req.input_id, req.stateless);
        if req.stateless {
            let tag = format!("{}.{}", req.output_id, req.output_tag);
            self.runner.watch_tag(req.input_id, tag);
        }
        Ok(Response::new(()))
    }

    async fn add_state_edge(
        &self, req: Request<AddStateEdgeRequest>,
    ) -> Result<Response<()>, Status> {
        let req = req.into_inner();
        info!("add_state_edge {}.{} -> {}.{}",
            req.output_id, req.output_tag, req.input_id, req.input_key);
        let mut hooks = self.runner.hooks.lock().unwrap();
        if let Some(hook) = hooks.get_mut(&req.output_id) {
            let state_perm = format!("{}.{}={}.{}",
                req.output_id, req.output_tag, req.input_id, req.input_key);
            hook.md.state_perm.push(state_perm);
            Ok(Response::new(()))
        } else {
            Err(Status::new(Code::NotFound, "module id not found"))
        }
    }

    async fn add_network_edge(
        &self, req: Request<AddNetworkEdgeRequest>,
    ) -> Result<Response<()>, Status> {
        let req = req.into_inner();
        info!("add_network_edge {} -> {}", req.output_id, req.domain);
        let mut hooks = self.runner.hooks.lock().unwrap();
        if let Some(hook) = hooks.get_mut(&req.output_id) {
            hook.md.network_perm.push(req.domain);
            Ok(Response::new(()))
        } else {
            Err(Status::new(Code::NotFound, "module id not found"))
        }
    }

    async fn persist_tag(
        &self, _req: Request<PersistTagRequest>,
    ) -> Result<Response<()>, Status> {
        unimplemented!()
    }

    async fn set_interval(
        &self, req: Request<SetIntervalRequest>,
    ) -> Result<Response<()>, Status> {
        let req = req.into_inner();
        info!("set_interval {} = {}s", req.hook_id, req.seconds);
        if self.runner.hooks.lock().unwrap().contains_key(&req.hook_id) {
            let duration = std::time::Duration::from_secs(req.seconds.into());
            self.runner.set_interval(req.hook_id, duration);
            Ok(Response::new(()))
        } else {
            Err(Status::new(Code::NotFound, "module id not found"))
        }
    }
}

impl Controller {
    /// Create a new controller.
    ///
    /// Call `start()` after constructing the controller to ensure it is
    /// listening for hosts and sensor requests.
    pub fn new(karl_path: PathBuf, password: &str, autoconfirm: bool) -> Self {
        Controller {
            karl_path: karl_path.clone(),
            scheduler: Arc::new(Mutex::new(HostScheduler::new(password))),
            data_sink: Arc::new(RwLock::new(DataSink::new(karl_path))),
            runner: HookRunner::new(),
            audit_log: Arc::new(Mutex::new(AuditLog::new())),
            sensors: Arc::new(Mutex::new(HashMap::new())),
            state: Arc::new(RwLock::new(HashMap::new())),
            autoconfirm,
        }
    }

    /// Initializes sensors based on the `<KARL_PATH>/clients.txt` file and
    /// starts the web dashboard.
    pub async fn start(
        &mut self,
        use_dashboard: bool,
        port: u16,
    ) -> Result<(), Error> {
        // Make the karl path if it doesn't already exist.
        fs::create_dir_all(&self.karl_path).unwrap();

        /*
        // Initialize sensors in the sensors file at `<KARL_PATH>/clients.txt`.
        // Expect sensor serialization format based on `dashboard/mod.rs`:
        // `<CLIENT_NAME>:<CLIENT_ADDR>=<CLIENT_TOKEN>`
        {
            let mut clients = self.sensors.lock().unwrap();
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
                    line[(j+1)..].to_string(),
                    Client {
                        confirmed: true,
                        id: line[..i].to_string(),
                        addr: line[(i+1)..j].parse().unwrap(),
                    }
                );
            }
        }
        */

        // Start the dashboard.
        if use_dashboard {
            dashboard::start(
                self.karl_path.clone(),
                self.scheduler.clone(),
                self.sensors.clone(),
            );
        }
        // Start the hook runner.
        self.runner.start(
            self.audit_log.clone(),
            self.scheduler.clone(),
            false,
        );

        info!("Karl controller listening on port {}", port);
        Ok(())
    }

    /// Register a hook, authenticating the client.
    ///
    /// If the provided client token does not correspond to a registered
    /// client, logs a warning message about an unauthorized client.
    ///
    /// IoError if error importing hook from filesystem.
    /// HookInstallError if environment variables are formatted incorrectly.
    /// AuthError if not a valid client token.
    fn register_hook(
        &self,
        _token: &SensorToken,
        global_hook_id: String,
    ) -> Result<HookID, Error> {
        // // Validate the client token.
        // if let Some(sensor) = self.sensors.lock().unwrap().get(token) {
        //     if !sensor.confirmed {
        //         println!("register_hook unconfirmed sensor token {:?}", token);
        //         return Err(Error::AuthError("invalid sensor token".to_string()));
        //     }
        // } else {
        //     println!("register_hook invalid sensor token {:?}", token);
        //     return Err(Error::AuthError("invalid sensor token".to_string()));
        // }

        // Register the hook.
        let (hook_id, tags) = self.runner.register_hook(global_hook_id)?;
        self.data_sink.write().unwrap().new_entity(hook_id.clone(), tags)?;
        Ok(hook_id)
    }

    /// Register a client.
    ///
    /// Stores the client-generated ID and controller-generated token
    /// along with socket information about the client. Registers the app
    /// (a Handlebars template) at `<KARL_PATH>/www/<CLIENT_ID>.hbs`. Creates
    /// an empty storage directory at `<KARL_PATH>/storage/<CLIENT_ID>/`,
    /// if it doesn't already exist.
    ///
    /// All characters in the client ID must be lowercase alphabet a-z,
    /// digits 0-9, or underscores. Lowercases the client ID, and removes
    /// other noncomplying characters. For example, "a @#Ld_e" becomes "alde".
    /// Resolves duplicate client IDs by appending an underscore and a number.
    /// e.g. camera -> camera_1 -> camera_2.
    ///
    /// Parameters:
    /// - id - The self-given ID of the client.
    /// - addr - The peer address of the TCP connection registering
    ///   the client.
    /// - keys - State keys.
    /// - tags - Output tags.
    /// - app_bytes - The bytes of the Handlebars template, or an empty
    ///   vector if there is no registered app.
    /// - confirmed - Whether the client should be confirmed by default.
    ///   Overriden by autoconfirm.
    fn sensor_register(
        &self,
        mut id: String,
        addr: IpAddr,
        keys: Vec<String>,
        tags: Vec<String>,
        app_bytes: &[u8],
        confirmed: bool,
    ) -> SensorRegisterResult {
        // resolve duplicate sensor ids
        let ids = self.sensors.lock().unwrap().values()
            .map(|sensor| sensor.id.clone()).collect::<HashSet<_>>();
        id = id.trim().to_lowercase();
        id = id
            .chars()
            .filter(|ch| ch.is_alphanumeric() || ch == &'_')
            .collect();
        if ids.contains(&id) {
            let mut i = 1;
            loop {
                let new_id = format!("{}_{}", id, i);
                if !ids.contains(&new_id) {
                    id = new_id;
                    break;
                }
                i += 1;
            }
        }

        // generate a sensor with a unique id and token
        let sensor = Client {
            confirmed: confirmed || self.autoconfirm,
            id: id.clone(),
            keys,
            tags,
            addr,
        };

        // register the sensor's webapp
        if !app_bytes.is_empty() {
            let parent = self.karl_path.join("www");
            let path = parent.join(format!("{}.hbs", &sensor.id));
            fs::create_dir_all(parent).unwrap();
            fs::write(&path, app_bytes).unwrap();
            info!("registered app ({} bytes) at {:?}", app_bytes.len(), path);
        }

        // register the sensor itself
        let mut sensors = self.sensors.lock().unwrap();
        let token = Token::gen();
        if sensors.insert(token.clone(), sensor.clone()).is_none() {
            info!(
                "registered sensor {} {} keys={:?} tags={:?}",
                sensor.id,
                token,
                sensor.keys,
                sensor.tags,
            );
        } else {
            unreachable!("impossible to generate duplicate sensor tokens")
        }
        SensorRegisterResult {
            sensor_token: token,
            sensor_id: id,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::net::SocketAddr;
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
    fn add_host_test(c: &mut Controller, i: usize) -> HostToken {
        let name = format!("host{}", i);
        let addr: SocketAddr = format!("0.0.0.0:808{}", i).parse().unwrap();
        let host_token = c.scheduler.lock().unwrap().add_host(name, addr, true, PASSWORD);
        assert!(host_token.is_some());
        host_token.unwrap()
    }

    /// Wrapper around register_hook to avoid rewriting parameters
    fn register_hook_test(
        c: &mut Controller,
        token: &SensorToken,
    ) -> Result<(), Error> {
        c.register_hook(token, "hello-world".to_string(), vec![], vec![], vec![], vec![])
    }

    #[tokio::test]
    async fn test_register_hook_invalid_sensor_token() {
        let (_karl_path, mut c) = init_test();

        // Register a client
        let client = c.sensor_register("name".to_string(), "0.0.0.0".parse().unwrap(), &vec![], true);
        let token = client.sensor_token.to_string();
        let bad_token1 = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789ab".to_string();
        let bad_token2 = "badtoken".to_string();

        // Add a host.
        c.runner.start(c.audit_log.clone(), c.scheduler.clone(), true);
        let host_token = add_host_test(&mut c, 1);
        c.scheduler.lock().unwrap().heartbeat(host_token.clone());
        assert!(register_hook_test(&mut c, &token).is_ok());
        c.scheduler.lock().unwrap().heartbeat(host_token.clone());
        assert!(register_hook_test(&mut c, &token).is_ok());
        c.scheduler.lock().unwrap().heartbeat(host_token.clone());
        assert!(register_hook_test(&mut c, &bad_token1).is_err());
        assert!(register_hook_test(&mut c, &bad_token2).is_err());
        assert!(register_hook_test(&mut c, &token).is_ok());
    }

    #[tokio::test]
    async fn test_register_hook_unconfirmed_sensor_token() {
        let (_karl_path, mut c) = init_test();

        // Add a host.
        c.runner.start(c.audit_log.clone(), c.scheduler.clone(), true);
        let host_token = add_host_test(&mut c, 1);
        c.scheduler.lock().unwrap().heartbeat(host_token);

        // Register an unconfirmed client
        let client = c.sensor_register("name".to_string(), "0.0.0.0".parse().unwrap(), &vec![], false);
        let token = client.sensor_token.to_string();
        assert_eq!(c.sensors.lock().unwrap().len(), 1);
        assert!(!c.sensors.lock().unwrap().get(&token).unwrap().confirmed);
        assert!(!register_hook_test(&mut c, &token).is_ok(),
            "found host with unconfirmed token");

        // Confirm the client and find a host.
        c.sensors.lock().unwrap().get_mut(&token).unwrap().confirmed = true;
        assert!(register_hook_test(&mut c, &token).is_ok(),
            "failed to find host with confirmed token");
    }

    #[test]
    fn test_sensor_register() {
        let (karl_path, c) = init_test();
        let storage_path = karl_path.path().join("storage").join("hello");
        let app_path = karl_path.path().join("www").join("hello.hbs");

        // Check initial conditions.
        assert!(c.sensors.lock().unwrap().is_empty());
        assert!(!storage_path.exists());
        assert!(!app_path.exists());

        // Register a client with an app.
        let id = "hello";
        let client_ip: IpAddr = "127.0.0.1".parse().unwrap();
        let app = vec![10, 10, 10, 10];
        c.sensor_register(id.to_string(), client_ip, &app, true);
        assert_eq!(c.sensors.lock().unwrap().len(), 1, "registered client");

        // Check the client was registered correcftly.
        let client = c.sensors.lock().unwrap().values().next().unwrap().clone();
        assert_eq!(client.id, id);
        assert_eq!(client.addr, client_ip);
        assert!(storage_path.is_dir(), "storage dir was not created");
        assert!(app_path.is_file(), "app was not created and renamed");

        // Register a client without an app.
        // Storage directory should be created, but app file should not.
        let app: Vec<u8> = vec![];
        c.sensor_register("world".to_string(), client_ip, &app, true);
        assert_eq!(c.sensors.lock().unwrap().len(), 2);
        assert!(karl_path.path().join("storage").join("world").is_dir());
        assert!(!karl_path.path().join("www").join("world.hbs").exists());
    }

    #[test]
    fn test_register_duplicate_clients() {
        let (karl_path, c) = init_test();
        let storage_base = karl_path.path().join("storage");
        let app_base = karl_path.path().join("www");

        // Register a client with an app.
        let id = "hello";
        let client_ip: IpAddr = "127.0.0.1".parse().unwrap();
        let app = vec![10, 10, 10, 10];
        c.sensor_register(id.to_string(), client_ip.clone(), &app, true);
        c.sensor_register(id.to_string(), client_ip.clone(), &app, true);
        c.sensor_register(id.to_string(), client_ip.clone(), &app, true);

        // Check the clients have different ids.
        let ids = c.sensors.lock().unwrap().values().map(|client| client.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids.len(), 3);
        assert!(ids.contains(&"hello".to_string()));
        assert!(ids.contains(&"hello_1".to_string()));
        assert!(ids.contains(&"hello_2".to_string()));

        // Check the clients all have registered storage and app paths.
        assert!(storage_base.join("hello").is_dir());
        assert!(storage_base.join("hello_1").is_dir());
        assert!(storage_base.join("hello_2").is_dir());
        assert!(app_base.join("hello.hbs").is_file());
        assert!(app_base.join("hello_1.hbs").is_file());
        assert!(app_base.join("hello_2.hbs").is_file());
    }

    #[test]
    fn test_register_format_ids() {
        let (_karl_path, c) = init_test();

        let client_ip: IpAddr = "127.0.0.1".parse().unwrap();
        c.sensor_register("   leadingwhitespace".to_string(), client_ip.clone(), &vec![], true);
        c.sensor_register("trailingwhitespace \t".to_string(), client_ip.clone(), &vec![], true);
        c.sensor_register("uPpErCaSe".to_string(), client_ip.clone(), &vec![], true);
        c.sensor_register("iNv@l1d_ch@r$".to_string(), client_ip.clone(), &vec![], true);
        c.sensor_register("\tEVERYth!ng   \n\r".to_string(), client_ip.clone(), &vec![], true);

        // Check the ids were formatted correctly (trimmed and lowercase).
        let ids = c.sensors.lock().unwrap().values().map(|client| client.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids.len(), 5);
        assert!(ids.contains(&"leadingwhitespace".to_string()));
        assert!(ids.contains(&"trailingwhitespace".to_string()));
        assert!(ids.contains(&"uppercase".to_string()));
        assert!(ids.contains(&"invl1d_chr".to_string()));
        assert!(ids.contains(&"everythng".to_string()));
    }

    #[test]
    fn test_sensor_register_result() {
        let (_karl_path, c) = init_test();
        let res1 = c.sensor_register("c1".to_string(), "1.0.0.0".parse().unwrap(), &vec![], true);
        let res2 = c.sensor_register("c2".to_string(), "2.0.0.0".parse().unwrap(), &vec![], true);
        let res3 = c.sensor_register("c3".to_string(), "3.0.0.0".parse().unwrap(), &vec![], true);

        // Tokens are unique
        assert!(res1.sensor_token != res2.sensor_token);
        assert!(res1.sensor_token != res3.sensor_token);
        assert!(res2.sensor_token != res3.sensor_token);
    }

    #[test]
    fn test_autoconfirm_client() {
        let dir1 = TempDir::new("karl").unwrap();
        let dir2 = TempDir::new("karl").unwrap();
        let c1 = Controller::new(dir1.path().to_path_buf(), PASSWORD, false);
        let c2 = Controller::new(dir2.path().to_path_buf(), PASSWORD, true);

        let id: String = "client_id".to_string();
        let addr: IpAddr = "1.2.3.4".parse().unwrap();
        c1.sensor_register(id.clone(), addr.clone(), &vec![], false);
        c2.sensor_register(id.clone(), addr.clone(), &vec![], false);
        let client1 = c1.sensors.lock().unwrap().values().next().unwrap().confirmed;
        let client2 = c2.sensors.lock().unwrap().values().next().unwrap().confirmed;
        assert!(!client1);
        assert!(client2);
    }
}
