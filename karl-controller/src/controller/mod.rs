mod scheduler;
mod data;
pub(crate) mod runner;
pub(crate) mod sensors;
pub(crate) mod tags;
pub use scheduler::HostScheduler;
pub use data::DataSink;
pub use runner::{Modules, Runner};
use sensors::Sensors;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;
use std::fs;

use tonic::{Request, Response, Status, Code};
use tokio::sync::mpsc;
use tokio::runtime::Handle;
use tokio_stream::wrappers::ReceiverStream;
use crate::protos::*;
use karl_common::*;

pub fn to_status(e: Error) -> Status {
    match e {
        Error::InvalidHostMessage(s) => Status::new(Code::Unauthenticated, s),
        e => Status::new(Code::Unknown, format!("{:?}", e)),
    }
}

/// Controller used for discovering available Karl services and coordinating
/// client requests among available services.
#[derive(Clone)]
pub struct Controller {
    pub handle: Handle,
    pub karl_path: PathBuf,
    /// Data structure for adding and allocating hosts
    pub scheduler: Arc<Mutex<HostScheduler>>,
    /// Data structure for managing sensor data.
    pub data_sink: Arc<RwLock<DataSink>>,
    /// Data structure for queueing and spawning processes from hooks.
    pub runner: Runner,
    pub modules: Arc<Mutex<Modules>>,
    pub watched_tags: Arc<RwLock<HashMap<Tag, Vec<ModuleID>>>>,
    /// Map from client token to client.
    ///
    /// Unique identifier for the sensor, known only by the controller
    /// and the sensor itself. Generated on registration. All host
    /// requests from the sensor to the controller must include this token.
    pub sensors: Arc<Mutex<Sensors>>,
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
        debug!("network domain={}", req.domain);
        Ok(Response::new(()))
    }

    /// This request is endorsed by the host. Simply read the data and return.
    async fn forward_get(
        &self, req: Request<GetData>,
    ) -> Result<Response<GetDataResult>, Status> {
        // TODO: validate host token
        let req = req.into_inner();
        info!("get_data tag={} {}-{}", req.tag, req.lower, req.upper);
        let res = self.data_sink.read().unwrap()
            .get_data(req.tag, req.lower, req.upper)
            .map_err(|e| to_status(e))?;
        Ok(Response::new(GetDataResult {
            data: res.data,
            timestamps: res.timestamps,
        }))
    }

    /// This request is endorsed by the host. Simply write data and return.
    async fn forward_push(
        &self, req: Request<PushData>,
    ) -> Result<Response<()>, Status> {
        // TODO: validate host token
        let req = req.into_inner();
        let label: KarlLabel = String::from("").into(); // TODO
        let res = self.data_sink.write().unwrap()
            .push_data(&req.tag, &req.data, label)
            .map_err(|e| to_status(e))?;
        // TODO: move to its own thread
        warn!("finish person_detection_pipeline (data persisted): {:?}", Instant::now());
        self.runner.spawn_module_if_watched(
            &res.modified_tag,
            &res.timestamp,
            &req.data,
        ).await;
        Ok(Response::new(()))
    }

    /// This request is endorsed by the host. Simply forward the state
    /// change and return.
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
            req.keys,
            req.returns.clone(),
            &req.app[..],
        );
        trace!("sensor_register => {} s", now.elapsed().as_secs_f32());
        Ok(Response::new(res))
    }

    /// Since the output node is a sensor, the key is a param and not a tag.
    /// The controller tracks which tags the param maps to and pushes to all
    /// the tags. It tells the runner to see if any modules were watching
    /// those tags.
    async fn push_raw_data(
        &self, req: Request<SensorPushData>,
    ) -> Result<Response<()>, Status> {
        let req = req.into_inner();
        let (sensor_id, tags) = {
            let sensors = self.sensors.lock().unwrap();
            if let Some(id) = sensors.authenticate(&req.sensor_token) {
                let tags = sensors.tags(&id).map_err(|e| e.to_tonic())?
                    .get_output_tags(&req.param).map_err(|e| e.to_tonic())?.clone();
                (id.clone(), tags.clone())
            } else {
                // drop messages from unconfirmed sensors
                return Ok(Response::new(()));
            }
        };
        for tag in tags {
            let res = self.data_sink.write().unwrap()
                .push_data(&tag, &req.data, sensor_id.clone().into())
                .map_err(|e| to_status(e))?;
            self.runner.spawn_module_if_watched(
                &res.modified_tag,
                &res.timestamp,
                &req.data,
            ).await;
        }
        Ok(Response::new(()))
    }

    type StateChangesStream = ReceiverStream<Result<StateChangePair, Status>>;

    async fn state_changes(
        &self, req: Request<StateChangeInit>,
    ) -> Result<Response<Self::StateChangesStream>, Status> {
        let req = req.into_inner();
        let sensor_id: String =
        loop {
            if let Some(id) = self.sensors.lock().unwrap().authenticate(&req.sensor_token) {
                break id.clone();
            }
            // poll until the sensor is confirmed
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
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
}

impl Controller {
    /// Create a new controller.
    ///
    /// Call `start()` after constructing the controller to ensure it is
    /// listening for hosts and sensor requests.
    pub fn new(
        handle: Handle,
        karl_path: PathBuf,
        password: &str,
        autoconfirm: bool,
        caching_enabled: bool,
        pubsub_enabled: bool,
    ) -> Self {
        let watched_tags = Arc::new(RwLock::new(HashMap::new()));
        Controller {
            handle: handle.clone(),
            karl_path: karl_path.clone(),
            scheduler: Arc::new(Mutex::new(HostScheduler::new(password, caching_enabled))),
            data_sink: Arc::new(RwLock::new(DataSink::new(karl_path))),
            runner: Runner::new(handle, pubsub_enabled, watched_tags.clone()),
            modules: Arc::new(Mutex::new(Modules::default())),
            watched_tags,
            sensors: Arc::new(Mutex::new(Sensors::default())),
            state: Arc::new(RwLock::new(HashMap::new())),
            autoconfirm,
        }
    }

    /// Initializes sensors based on the `<KARL_PATH>/clients.txt` file.
    pub async fn start(&mut self, port: u16) -> Result<(), Error> {
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

        // Start the hook runner.
        self.runner.start(self.modules.clone(), self.scheduler.clone(), false);

        info!("Karl controller listening on port {}", port);
        Ok(())
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
        keys: Vec<String>,
        returns: Vec<String>,
        app_bytes: &[u8],
    ) -> SensorRegisterResult {
        // generate a sensor with a unique id and token
        let mut sensors = self.sensors.lock().unwrap();
        id = sensors.unique_id(id);
        let sensor = sensors::Sensor {
            confirmed: self.autoconfirm,
            id: id.clone(),
            keys,
            returns,
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
        let token = Token::gen();
        sensors.add_sensor(sensor, token.clone());
        SensorRegisterResult {
            sensor_token: token,
            sensor_id: id,
        }
    }

    pub fn add_module(
        &self,
        global_module_id: &String,
        module_id: &String,
        modules: &mut Modules,
    ) -> Result<(), Error> {
        debug!("add module {} ({})", module_id, global_module_id);
        modules.add_module(global_module_id, module_id).map_err(|e| {
            error!("error adding module: {:?}", e);
            Error::BadRequest
        })?;
        Ok(())
    }

    /// Removes the module if all edges have been removed,
    /// and all intervals and network edges have been removed.
    pub fn remove_module(
        &self,
        module_id: String,
        modules: &mut Modules,
    ) -> Result<(), Error> {
        debug!("remove module {}", module_id);
        modules.remove_module(module_id)?;
        Ok(())

    }

    /// Assigns a tag name to the input node, if it doesn't already have one.
    /// Adds that tag name to the output node. If the output node is a sensor,
    /// track the return mapping for that output node. If the edge is stateless,
    /// then tell the runner to watch the tag.
    ///
    /// Updates how new data labels are assigned relative to the new graph.
    pub fn add_data_edge(
        &self,
        stateless: bool,
        src_id: String,
        src_name: String,
        dst_id: String,
        dst_name: String,
        modules: &mut Modules,
        sensors: &mut Sensors,
    ) -> Result<(), Error> {
        debug!("add data edge {}.{} -> {}.{} stateless={}",
            src_id, src_name, dst_id, dst_name, stateless);
        // assign a tag name to the input node, if it doesn't already have one.
        let tag = {
            if let Some(maybe_tag) = modules.tags_mut(&dst_id)?.get_input_tag(&dst_name)? {
                maybe_tag.clone()
            } else {
                let tag = modules.next_tag();
                modules.tags_mut(&dst_id)?.set_input_tag(&dst_name, &tag)?;
                tag
            }
        };

        // add that tag name to the output node.
        if modules.module_exists(&src_id) {
            modules.tags_mut(&src_id)?.add_output_tag(&src_name, &tag)?;
        } else if sensors.sensor_exists(&src_id) {
            sensors.tags_mut(&src_id)?.add_output_tag(&src_name, &tag)?;
        } else {
            error!("output sensor nor module exists");
            return Err(Error::NotFound);
        }

        // if the edge is stateless, tell the runner to watch that tag.
        if stateless {
            self.runner.watch_tag(dst_id, tag.to_string());
        }
        Ok(())
    }

    /// Removes the association from the return value to the tag for the
    /// edge. Does not remove the input tag if there are no more incoming
    /// edges. If stateless, removes the watched tag.
    pub fn remove_data_edge(
        &self,
        stateless: bool,
        src_id: String,
        src_name: String,
        dst_id: String,
        dst_name: String,
        modules: &mut Modules,
        sensors: &mut Sensors,
    ) -> Result<(), Error> {
        debug!("remove data edge {}.{} -> {}.{} stateless={}",
            src_id, src_name, dst_id, dst_name, stateless);
        let tag = {
            if let Some(tag) = modules.tags(&dst_id)?.get_input_tag(&dst_name)? {
                tag.clone()
            } else {
                debug!("input tag {}.{} is not set, meaning edges does not exist",
                    dst_id, dst_name);
                return Err(Error::NotFound);
            }
        };

        if modules.module_exists(&src_id) {
            modules.tags_mut(&src_id)?.remove_output_tag(&src_name, &tag)?;
        } else if sensors.sensor_exists(&src_id) {
            sensors.tags_mut(&src_id)?.remove_output_tag(&src_name, &tag)?;
        } else {
            error!("output sensor nor module exists");
            return Err(Error::NotFound);
        }

        if stateless {
            self.runner.unwatch_tag(dst_id, &tag)?;
        }
        Ok(())
    }

    /// Adds the unique tag name associated with sensor state to the output
    /// node.
    ///
    /// Updates how new data labels are assigned relative to the new graph.
    pub fn add_state_edge(
        &self,
        src_id: String,
        src_name: String,
        sensor_id: String,
        sensor_key: String,
        modules: &mut Modules,
        sensors: &Sensors,
    ) -> Result<(), Error> {
        debug!("add state_edge {}.{} -> {}.{}",
            src_id, src_name, sensor_id, sensor_key);
        // check that the sensor has that key
        if let Some(sensor) = sensors.get_sensor(&sensor_id) {
            if !sensor.keys.contains(&sensor_key) {
                error!("sensor does not have input key {:?}", sensor.keys);
                return Err(Error::NotFound);
            }
        } else {
            error!("output sensor does not exist");
            return Err(Error::NotFound);
        }

        // add the state tag to the output module
        let state_tag = tags::to_state_tag(&sensor_id, &sensor_key);
        modules.tags_mut(&src_id)?.add_output_tag(&src_name, &state_tag)?;
        Ok(())
    }

    pub fn remove_state_edge(
        &self,
        src_id: String,
        src_name: String,
        sensor_id: String,
        sensor_key: String,
        modules: &mut Modules,
    ) -> Result<(), Error> {
        debug!("remove state_edge {}.{} -> {}.{}",
            src_id, src_name, sensor_id, sensor_key);
        let state_tag = tags::to_state_tag(&sensor_id, &sensor_key);
        modules.tags_mut(&src_id)?.remove_output_tag(&src_name, &state_tag)?;
        Ok(())
    }

    /// Permits a specific module to access the network.
    /// Updates how data labels are assigned relative to the new graph.
    pub fn set_network_edges(
        &self,
        module_id: String,
        domains: Vec<String>,
        modules: &mut Modules,
    ) -> Result<(), Error> {
        debug!("network_edge {} -> {:?}", module_id, domains);
        modules.config_mut(&module_id)?.set_network_perm(domains);
        Ok(())
    }

    /// Set the module interval, aborting any previous interval threads
    /// if they already exist.
    pub fn set_interval(
        &self,
        module_id: String,
        duration: Option<u32>,
        modules: &mut Modules,
    ) -> Result<(), Error> {
        debug!("interval {} -> {:?}", module_id, duration);
        self.runner.set_interval(module_id, duration, modules)?;
        Ok(())
    }

}

/*
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
*/