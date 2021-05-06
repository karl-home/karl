#![feature(proc_macro_hygiene, decl_macro)]
#![feature(custom_inner_attributes)]

#[macro_use]
extern crate log;

pub mod net;
mod runtime;
mod cache;
mod perms;
use cache::PathManager;
use perms::ProcessPerms;

pub mod protos {
	tonic::include_proto!("request");
}
use protos::karl_host_server::KarlHostServer;
use protos::*;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};

use tokio;
use karl_common::*;
use reqwest::{self, Method, header::HeaderName};
use tonic::{Request, Response, Status, Code};
use tonic::transport::Server;
use clap::{Arg, App};

pub struct Host {
    /// Host ID (unique among hosts)
    id: u32,
    /// Host API to controller.
    api: crate::net::KarlHostAPI,
    /// Active process tokens.
    process_tokens: Arc<Mutex<HashMap<ProcessToken, ProcessPerms>>>,
    /// Path manager.
    path_manager: Arc<PathManager>,
    /// Only one compute at a time.
    compute_lock: Arc<Mutex<()>>,
    /// Whether caching is diabled
    caching_disabled: bool,
    /// Whether to mock network access
    mock_network: bool,
}

#[tonic::async_trait]
impl karl_host_server::KarlHost for Host {
    async fn start_compute(
        &self, req: Request<ComputeRequest>,
    ) -> Result<Response<NotifyStart>, Status> {
        let mut req = req.into_inner();
        info!("HANDLE_COMPUTE START {}", req.hook_id);
        let process_token = Token::gen();
        let perms = ProcessPerms::new(&mut req);

        // Mark an active process
        let mut process_tokens = self.process_tokens.lock().unwrap();
        assert!(!process_tokens.contains_key(&process_token));
        process_tokens.insert(process_token.clone(), perms);
        info!("starting process {}", &process_token);

        // Handle the process asynchronously
        {
            let binary_path = Path::new(&req.binary_path).to_path_buf();
            let api = self.api.clone();
            let path_manager = self.path_manager.clone();
            let process_tokens = self.process_tokens.clone();
            let process_token = process_token.clone();
            let compute_lock = self.compute_lock.clone();
            let caching_disabled = self.caching_disabled;
            tokio::spawn(async move {
                if !req.triggered_tag.is_empty() {
                    req.envs.push(format!("TRIGGERED_TAG={}", &req.triggered_tag));
                }
                if !req.triggered_timestamp.is_empty() {
                    req.envs.push(format!("TRIGGERED_TIMESTAMP={}", &req.triggered_timestamp));
                }
                req.envs.push(format!("PROCESS_TOKEN={}", &process_token));
                if !req.params.is_empty() {
                    req.envs.push(format!("KARL_PARAMS={}", &req.params));
                }
                if !req.returns.is_empty() {
                    req.envs.push(format!("KARL_RETURNS={}", &req.returns));
                }
                Host::handle_compute(
                    compute_lock,
                    path_manager,
                    req.hook_id,
                    req.cached,
                    caching_disabled,
                    req.package,
                    binary_path,
                    req.args,
                    req.envs,
                ).unwrap();
                process_tokens.lock().unwrap().remove(&process_token);
                api.notify_end(process_token).await.unwrap();
            });
        }

        Ok(Response::new(NotifyStart { process_token }))
    }

    async fn network(
        &self, req: Request<NetworkAccess>,
    ) -> Result<Response<NetworkAccessResult>, Status> {
        // Validate the process is valid and has permissions to access the network.
        // No serializability guarantees from other requests from the same process.
        // Sanitizes the path.
        let req = req.into_inner();
        if let Some(perms) = self.process_tokens.lock().unwrap().get(&req.process_token) {
            if !perms.can_access_domain(&req.domain) {
                return Err(Status::new(Code::Unauthenticated, "invalid network access"));
            }
        } else {
            return Err(Status::new(Code::Unauthenticated, "invalid process token"));
        }

        // Build the network request
        let method = match req.method.as_str() {
            "GET" => Method::GET,
            "POST" => Method::POST,
            "PUT" => Method::PUT,
            "DELETE" => Method::DELETE,
            "HEAD" => Method::HEAD,
            "OPTIONS" => Method::OPTIONS,
            "CONNECT" => Method::CONNECT,
            "PATCH" => Method::PATCH,
            "TRACE" => Method::TRACE,
            _ => { return Err(Status::new(Code::InvalidArgument, "invalid http method")) },
        };
        let mut builder = reqwest::Client::new()
            .request(method, req.domain.clone())
            .timeout(Duration::from_secs(1))
            .body(req.body.clone());
        for header in &req.headers {
            let key = HeaderName::from_bytes(&header.key[..])
                .map_err(|e| {
                    error!("{}", e);
                    Status::new(Code::InvalidArgument, "invalid header name")
                })?;
            builder = builder.header(key, &header.value[..]);
        }


        if self.mock_network {
            // Forward the network access to the controller.
            warn!("mock network request");
            self.api.forward_network(req).await?;
            Ok(Response::new(NetworkAccessResult::default()))
        } else {
            // Make the actual network access
            let handle = tokio::spawn(async move {
                let res = builder.send().await.map_err(|e| {
                    error!("{}", e);
                    Status::new(Code::Aborted, "http request failed")
                })?;
                let status_code = res.status().as_u16() as u32;
                let headers = res
                    .headers()
                    .iter()
                    .map(|(key, value)| KeyValuePair {
                        key: key.as_str().as_bytes().to_vec(),
                        value: value.as_bytes().to_vec(),
                    })
                    .collect::<Vec<_>>();
                let data = res.bytes().await.map_err(|e| {
                    error!("{}", e);
                    Status::new(Code::Unavailable, "error streaming response bytes")
                })?;
                Ok(Response::new(NetworkAccessResult {
                    status_code,
                    headers,
                    data: data.to_vec(),
                }))
            });

            // Forward the network access to the controller.
            self.api.forward_network(req).await?;

            // Return the result of the HTTP request.
            handle.await.map_err(|e| Status::new(Code::Internal, format!("{}", e)))?
        }
    }

    /// Validates the process is an existing process, and checks its
    /// permissions to see that the tag corresponds to a valid param.
    /// If the tag is valid and this is a stateless edge, only respond
    /// succesfully if the module is trying to get the triggered data.
    /// If the tag is valid and this is a stateful edge, endorse the data
    /// with the host token and forward to the controller.
    async fn get(
        &self, req: Request<GetData>,
    ) -> Result<Response<GetDataResult>, Status> {
        // Validate the process is valid and has permissions to read the tag.
        // No serializability guarantees from other requests from the same process.
        let req = req.into_inner();
        if let Some(perms) = self.process_tokens.lock().unwrap().get_mut(&req.process_token) {
            if perms.is_triggered(&req.tag) {
                // cached the triggered file
                let res = if req.lower != req.upper {
                    debug!("get: {} invalid triggered timestamps", req.process_token);
                    GetDataResult::default()
                } else if let Some(data) = perms.read_triggered(&req.lower) {
                    debug!("get: {} reading triggered data", req.process_token);
                    GetDataResult {
                        timestamps: vec![req.lower],
                        data: vec![data],
                    }
                } else {
                    debug!("get: {} process was not triggered", req.process_token);
                    GetDataResult::default()
                };
                return Ok(Response::new(res));
            }
            if !perms.can_read(&req.tag) {
                warn!("get: {} cannot read {}", req.process_token, req.tag);
                return Err(Status::new(Code::Unauthenticated, "cannot read"));
            }
        } else {
            warn!("get: invalid token {}", req.process_token);
            return Err(Status::new(Code::Unauthenticated, "invalid process token"));
        }
        // Forward the file access to the controller and return the result
        debug!("get: {} forwarding tag={}", req.process_token, req.tag);
        self.api.forward_get(req).await
    }

    /// Validates the process is an existing process, and checks its
    /// permissions to see that the process is writing to a valid tag.
    /// If the tag is valid, endorse the data with the host token and
    /// forward to the controller.
    ///
    /// If the tag corresponds to sensor state (say maybe it starts with #
    /// which is reserved for state tags), forward the request as a state
    /// change instead.
    async fn push(
        &self, req: Request<PushData>,
    ) -> Result<Response<()>, Status> {
        // Validate the process is valid and has permissions to write the file.
        // No serializability guarantees from other requests from the same process.
        // Sanitizes the path.
        let req = req.into_inner();
        let sensor_key = if let Some(perms) = self.process_tokens.lock().unwrap().get(&req.process_token) {
            if !perms.can_write(&req.tag) {
                debug!("push: {} cannot write tag={}, silently failing", req.process_token, req.tag);
                return Ok(Response::new(()));
            }
            if req.tag.chars().next() == Some('#') {
                let mut split = req.tag.split(".");
                let sensor = split.next().unwrap();
                let key = split.next().unwrap();
                Some((sensor[1..].to_string(), key.to_string()))
            } else {
                None
            }
        } else {
            return Err(Status::new(Code::Unauthenticated, "invalid process token"));
        };

        if let Some((sensor, key)) = sensor_key {
            // Forward as state change if the tag changes state.
            debug!("push: {} forwarding state change tag={}", req.process_token, req.tag);
            let req = StateChange {
                host_token: String::new(),
                process_token: req.process_token,
                sensor_id: sensor,
                key,
                value: req.data,
            };
            self.api.forward_state(req).await
        } else {
            // Forward the file access to the controller and return the result
            debug!("push: {} forwarding push tag={}", req.process_token, req.tag);
            self.api.forward_push(req).await
        }
    }
}

impl Host {
    /// Generate a new host with a random ID.
    pub fn new(
        karl_path: PathBuf,
        controller: &str,
        caching_disabled: bool,
        mock_network: bool,
    ) -> Self {
        use rand::Rng;
        let id: u32 = rand::thread_rng().gen();
        Self {
            id,
            api: crate::net::KarlHostAPI::new(controller),
            process_tokens: Arc::new(Mutex::new(HashMap::new())),
            path_manager: Arc::new(PathManager::new(karl_path, id)),
            compute_lock: Arc::new(Mutex::new(())),
            caching_disabled,
            mock_network,
        }
    }

    /// Spawns a background process that sends heartbeats to the controller
    /// at the HEARTBEAT_INTERVAL.
    ///
    /// The constructor creates a directory at the <KARL_PATH> if it does
    /// not already exist. The working directory for any computation is at
    /// <KARL_PATH>/<LISTENER_ID>. When not doing computation, the working
    /// directory must be at <KARL_PATH>.
    ///
    /// Parameters:
    /// - port - The port to listen on.
    /// - password - The password to register with the controller.
    pub async fn start(&mut self, port: u16, password: &str) -> Result<(), Status> {
        self.api.register(self.id, port, password).await?;
        let api = self.api.clone();
        tokio::spawn(async move {
            // Every HEARTBEAT_INTERVAL seconds, this process wakes up
            // sends a heartbeat message to the controller.
            loop {
                tokio::time::sleep(Duration::from_secs(HEARTBEAT_INTERVAL)).await;
                trace!("heartbeat");
                let res = api.heartbeat().await;
                if let Err(e) = res {
                    warn!("error sending heartbeat: {}", e);
                };
            }
        });
        Ok(())
    }

    /// Handle a compute request.
    ///
    /// The client must be verified by the caller.
    fn handle_compute(
        lock: Arc<Mutex<()>>,
        path_manager: Arc<PathManager>,
        hook_id: HookID,
        cached: bool,
        caching_disabled: bool,
        package: Vec<u8>,
        binary_path: PathBuf,
        args: Vec<String>,
        envs: Vec<String>,
    ) -> Result<(), Error> {
        let now = Instant::now();
        if cached && caching_disabled {
            return Err(Error::CacheError("caching is disabled".to_string()));
        }
        // TODO: lock on finer granularity, just the specific module
        // But gets a lock around the filesystem so multiple people
        // aren't handling compute requests that could be cached.
        // And so that each request can create a directory for its process.
        let (mount, paths) = {
            let lock = lock.lock().unwrap();
            debug!("cached={} caching_disabled={}", cached, caching_disabled);
            if cached && !path_manager.is_cached(&hook_id) {
                // TODO: controller needs to handle this error
                // what if a second request gets here before the first
                // request caches the module? race condition
                return Err(Error::CacheError(format!("hook {} is not cached", hook_id)));
            }
            if !cached && !caching_disabled {
                path_manager.cache_hook(&hook_id, package)?;
            }
            debug!("unpacked request => {} s", now.elapsed().as_secs_f32());
            let now = Instant::now();
            let (mount, paths) = path_manager.new_request(&hook_id);
            // info!("=> preprocessing: {} s", now.elapsed().as_secs_f32());
            debug!("mounting overlayfs => {} s", now.elapsed().as_secs_f32());
            drop(lock);
            (mount, paths)
        };

        let now = Instant::now();
        let _res = runtime::run(
            &paths.root_path,
            binary_path,
            args,
            envs,
        )?;
        debug!("invoked binary => {} s", now.elapsed().as_secs_f32());
        info!("HANDLE_COMPUTE FINISH {}", hook_id);

        // Reset the root for the next computation.
        path_manager.unmount(mount);
        if let Err(e) = std::fs::remove_dir_all(&paths.request_path) {
            error!("error resetting request path: {:?}", e);
        }
        let now = Instant::now();
        trace!(
            "reset directory at {:?} => {} s",
            &paths.request_path,
            now.elapsed().as_secs_f32(),
        );
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::builder().init();
    let matches = App::new("Karl Host")
        .arg(Arg::with_name("karl-path")
            .help("Absolute path to the base Karl directory.")
            .long("karl-path")
            .takes_value(true)
            .default_value("/home/gina/.karl"))
        .arg(Arg::with_name("port")
            .help("Port. Defaults to a random open port.")
            .short("p")
            .long("port")
            .takes_value(true)
            .default_value("59583"))
        .arg(Arg::with_name("password")
            .help("Controller password to register host.")
            .long("password")
            .takes_value(true)
            .default_value("password"))
        .arg(Arg::with_name("controller-ip")
            .help("IP address of the controller")
            .long("controller-ip")
            .takes_value(true)
            .default_value("127.0.0.1"))
        .arg(Arg::with_name("controller-port")
            .help("Port of the controller")
            .long("controller-port")
            .takes_value(true)
            .default_value("59582"))
        .arg(Arg::with_name("caching-disabled")
            .help("If the flag is included, disables caching.")
            .long("caching-disabled"))
        .arg(Arg::with_name("no-mock-network")
            .help("If the flag is included, uses the real network.")
            .long("no-mock-network"))
        .get_matches();

    let karl_path = Path::new(matches.value_of("karl-path").unwrap()).to_path_buf();
    let port: u16 = matches.value_of("port").unwrap().parse().unwrap();
    let controller = format!(
        "http://{}:{}",
        matches.value_of("controller-ip").unwrap(),
        matches.value_of("controller-port").unwrap(),
    );
    let password = matches.value_of("password").unwrap();
    let caching_disabled = matches.is_present("caching-disabled");
    let mock_network = !matches.is_present("no-mock-network");
    let mut host = Host::new(
        karl_path,
        &controller,
        caching_disabled,
        mock_network,
    );
    host.start(port, password).await.unwrap();
    Server::builder()
        .add_service(KarlHostServer::new(host))
        .serve(format!("0.0.0.0:{}", port).parse()?)
        .await
        .unwrap();
    Ok(())
}
