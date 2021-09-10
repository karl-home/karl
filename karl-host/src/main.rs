#![feature(proc_macro_hygiene, decl_macro)]
#![feature(custom_inner_attributes)]

#[macro_use]
extern crate log;

pub mod net;
mod runtime;
mod cache;
mod perms;
use cache::PathManager;
use perms::*;

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
use tokio::sync::mpsc;
use karl_common::*;
use reqwest::{self, Method, header::HeaderName};
use tonic::{Request, Response, Status, Code};
use tonic::transport::Server;
use clap::{Arg, App};

struct WarmProcess {
    process_token: ProcessToken,
    tx: mpsc::Sender<()>,
}

#[derive(Clone)]
pub struct Host {
    /// Host ID (unique among hosts)
    id: u32,
    /// Host API to controller.
    api: crate::net::KarlHostAPI,
    /// Active process tokens.
    process_tokens: Arc<Mutex<HashMap<ProcessToken, ProcessPerms>>>,
    warm_processes: Arc<Mutex<HashMap<ModuleID, Vec<WarmProcess>>>>,
    warm_cache_tx: Option<mpsc::Sender<ComputeRequest>>,
    /// Path manager.
    path_manager: Arc<PathManager>,
    /// Only one compute at a time.
    compute_lock: Arc<Mutex<()>>,
    /// Whether caching is enabled
    cold_cache_enabled: bool,
    warm_cache_enabled: bool,
    /// Whether to read triggered data locally or forward to the data sink
    pubsub_enabled: bool,
    /// Whether to mock network access
    mock_network: bool,
}

#[tonic::async_trait]
impl karl_host_server::KarlHost for Host {
    async fn start_compute(
        &self, req: Request<ComputeRequest>,
    ) -> Result<Response<NotifyStart>, Status> {
        let mut req = req.into_inner();
        trace!("HANDLE_COMPUTE START {}", req.module_id);
        if let Some(process_token) = self.attach_warm_process(&mut req).await {
            Ok(Response::new(NotifyStart { process_token }))
        } else {
            let is_warm = false;
            let process_token = Host::spawn_new_process(
                self.clone(),
                req,
                is_warm,
            ).await;
            Ok(Response::new(NotifyStart { process_token }))
        }
    }

    async fn network(
        &self, req: Request<NetworkAccess>,
    ) -> Result<Response<NetworkAccessResult>, Status> {
        debug!("network");
        // Validate the process is valid and has permissions to access the network.
        // No serializability guarantees from other requests from the same process.
        // Sanitizes the path.
        let req = req.into_inner();
        let rx = {
            if let Some(perms) = self.process_tokens.lock().unwrap().get_mut(&req.process_token) {
                perms.touch()
            } else {
                return Err(Status::new(Code::Unauthenticated, "invalid process token"));
            }
        };
        if let Some(mut rx) = rx {
            debug!("warm process awaiting...");
            rx.recv().await;
        }
        if let Some(perms) = self.process_tokens.lock().unwrap().get_mut(&req.process_token) {
            if !perms.can_access_domain(&req.domain) {
                return Err(Status::new(Code::Unauthenticated, "invalid network access"));
            }
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
            warn!("finish diff_priv_pipeline (statistics sent): {:?}", Instant::now());
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
        debug!("get");
        // Validate the process is valid and has permissions to read the tag.
        // No serializability guarantees from other requests from the same process.
        let req = req.into_inner();
        let rx = {
            if let Some(perms) = self.process_tokens.lock().unwrap().get_mut(&req.process_token) {
                perms.touch()
            } else {
                warn!("get: invalid token {}", req.process_token);
                return Err(Status::new(Code::Unauthenticated, "invalid process token"));
            }
        };
        if let Some(mut rx) = rx {
            debug!("warm process awaiting...");
            rx.recv().await;
        }
        if let Some(perms) = self.process_tokens.lock().unwrap().get_mut(&req.process_token) {
            if perms.is_triggered(&req.tag) {
                warn!("get: {} cannot read triggered tag {}", req.process_token, req.tag);
                return Err(Status::new(Code::Unauthenticated, "cannot read"));
            } else if !perms.can_read(&req.tag) {
                warn!("get: {} cannot read {}", req.process_token, req.tag);
                return Err(Status::new(Code::Unauthenticated, "cannot read"));
            }
        }
        // Forward the file access to the controller and return the result
        debug!("get: {} forwarding tag={}", req.process_token, req.tag);
        self.api.forward_get(req).await
    }

    /// Validates the process is an existing process, and checks its
    /// permissions to see that the tag corresponds to a valid param.
    /// If the tag is valid and this is a stateless edge, only respond
    /// succesfully if the module is trying to get the triggered data.
    /// If the tag is valid and this is a stateful edge, endorse the data
    /// with the host token and forward to the controller.
    async fn get_event(
        &self, req: Request<GetEventData>,
    ) -> Result<Response<GetDataResult>, Status> {
        debug!("get_event");
        // Validate the process is valid and has permissions to read the tag.
        // No serializability guarantees from other requests from the same process.
        let req = req.into_inner();
        let rx = {
            if let Some(perms) = self.process_tokens.lock().unwrap().get_mut(&req.process_token) {
                perms.touch()
            } else {
                warn!("get: invalid token {}", req.process_token);
                return Err(Status::new(Code::Unauthenticated, "invalid process token"));
            }
        };
        if let Some(mut rx) = rx {
            debug!("warm process awaiting...");
            rx.recv().await;
        }
        let (tag, timestamp) = if let Some(perms) = self.process_tokens.lock().unwrap().get_mut(&req.process_token) {
            if perms.is_triggered(&req.tag) {
                // cached the triggered file
                if !self.pubsub_enabled {
                    debug!("get: {} pubsub disabled, fallthrough to read from data sink", req.process_token);
                    // fallthrough below
                    (perms.triggered_tag.clone(), perms.triggered_timestamp.clone())
                } else if let Some(data) = perms.read_triggered() {
                    debug!("get: {} reading triggered data", req.process_token);
                    return Ok(Response::new(GetDataResult {
                        timestamps: vec![perms.triggered_timestamp.clone()],
                        data: vec![data],
                    }))
                } else {
                    debug!("get: {} process was not triggered", req.process_token);
                    return Ok(Response::new(GetDataResult::default()))
                }
            } else {
                warn!("get: {} cannot read non-triggered tag {}", req.process_token, req.tag);
                return Err(Status::new(Code::Unauthenticated, "cannot read"));
            }
        } else {
            unreachable!()
        };
        // Forward the file access to the controller and return the result
        debug!("get: {} forwarding tag={}", req.process_token, req.tag);
        self.api.forward_get(GetData {
            host_token: "".to_string(),
            process_token: req.process_token,
            tag: tag,
            lower: timestamp.clone(),
            upper: timestamp,
        }).await
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
        debug!("push");
        // Validate the process is valid and has permissions to write the file.
        // No serializability guarantees from other requests from the same process.
        // Sanitizes the path.
        let req = req.into_inner();
        let rx = {
            if let Some(perms) = self.process_tokens.lock().unwrap().get_mut(&req.process_token) {
                perms.touch()
            } else {
                return Err(Status::new(Code::Unauthenticated, "invalid process token"));
            }
        };
        if let Some(mut rx) = rx {
            debug!("warm process awaiting...");
            rx.recv().await;
        }
        if let Some(perms) = self.process_tokens.lock().unwrap().get_mut(&req.process_token) {
            if !perms.can_write(&req.tag) {
                debug!("push: {} cannot write tag={}, silently failing", req.process_token, req.tag);
                return Ok(Response::new(()));
            }
        } else {
            unreachable!()
        };
        // Forward the file access to the controller and return the result
        debug!("push: {} forwarding push tag={}", req.process_token, req.tag);
        self.api.forward_push(req).await
    }
}

impl Host {
    /// Generate a new host with a random ID.
    pub fn new(
        base_path: PathBuf,
        controller: &str,
        cold_cache_enabled: bool,
        warm_cache_enabled: bool,
        pubsub_enabled: bool,
        mock_network: bool,
    ) -> Self {
        use rand::Rng;
        let id: u32 = rand::thread_rng().gen();
        assert!(cold_cache_enabled || !warm_cache_enabled);
        // TODO: buffer size
        Self {
            id,
            api: crate::net::KarlHostAPI::new(controller),
            process_tokens: Arc::new(Mutex::new(HashMap::new())),
            warm_processes: Arc::new(Mutex::new(HashMap::new())),
            warm_cache_tx: None, // wish this didn't have to be wrapped
            path_manager: Arc::new(PathManager::new(base_path, id)),
            compute_lock: Arc::new(Mutex::new(())),
            cold_cache_enabled,
            warm_cache_enabled,
            pubsub_enabled,
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

        // listener for spawning warm processes
        let (tx, mut rx) = mpsc::channel::<ComputeRequest>(100);
        self.warm_cache_tx = Some(tx);
        let host = self.clone();
        tokio::spawn(async move {
            loop {
                let req: ComputeRequest = rx.recv().await.unwrap();
                let is_warm = true;
                Host::spawn_new_process(
                    host.clone(),
                    req,
                    is_warm,
                ).await;
            }
        });
        Ok(())
    }

    async fn attach_warm_process(
        &self,
        req: &mut ComputeRequest,
    ) -> Option<ProcessToken> {
        let warm_process = {
            let mut warm_processes = self.warm_processes.lock().unwrap();
            let mut process_tokens = self.process_tokens.lock().unwrap();
            let mut process: Option<WarmProcess> = None;
            if let Some(processes) = warm_processes.get_mut(&req.module_id) {
                // reserve the process token
                process = processes.pop();
            }
            if let Some(process) = process {
                process_tokens
                    .get_mut(&process.process_token).unwrap()
                    .set_compute_request(req);
                process
            } else {
                return None;
            }
        };
        // permissions are set and warm process can continue
        info!("attaching: {} ({})", req.module_id, warm_process.process_token);
        warm_process.tx.send(()).await.unwrap();
        Some(warm_process.process_token)
    }

    async fn spawn_new_process(
        host: Host,
        mut req: ComputeRequest,
        is_warm: bool,
    ) -> ProcessToken {
        let process_token = Token::gen();

        let (perms, tx) = if !is_warm {
            info!("spawning cold process: {} ({})", req.module_id, process_token);
            (ProcessPerms::new(&mut req), None)
        } else {
            info!("spawning warm process: {} ({})", req.module_id, process_token);
            let (perms, tx) = ProcessPerms::new_warm_cache();
            (perms, Some(tx))
        };

        // Mark an active process
        {
            let mut process_tokens = host.process_tokens.lock().unwrap();
            assert!(!process_tokens.contains_key(&process_token));
            process_tokens.insert(process_token.clone(), perms);
        }

        // If it's warm insert a sending channel to eventually notify
        // this process it is ready to continue
        if let Some(tx) = tx {
            host.warm_processes.lock().unwrap()
                .entry(req.module_id.clone())
                .or_insert(vec![])
                .push(WarmProcess {
                    process_token: process_token.clone(),
                    tx,
                });
        }

        // Handle the process asynchronously
        #[cfg(target_os = "linux")]
        {
            let binary_path = Path::new(&req.binary_path).to_path_buf();
            let process_token = process_token.clone();
            tokio::spawn(async move {
                let original_req = req.clone();
                req.envs.push(format!("PROCESS_TOKEN={}", &process_token));
                let execution_time = Host::handle_compute(
                    host.compute_lock.clone(),
                    host.path_manager.clone(),
                    req.module_id,
                    req.cached,
                    host.cold_cache_enabled,
                    req.package,
                    binary_path,
                    req.args,
                    req.envs,
                ).unwrap();
                host.process_tokens.lock().unwrap().remove(&process_token);
                host.api.notify_end(process_token).await.unwrap();

                // Now that the compute request is finished, evaluate its
                // initialization time. If the initialization time was high,
                // recursively call this function but as a warm cache module.
                // We assume initialization time is high if the warm cache
                // is enabled.
                let _long_init_time = execution_time > Duration::from_secs(5);
                if host.warm_cache_enabled {
                    debug!("execution_time was {:?}, spawning warm modules anyway", execution_time);
                    host.warm_cache_tx.as_ref().unwrap().send(original_req).await.unwrap();
                }
            });
        }
        #[cfg(not(target_os = "linux"))]
        {
            unimplemented!()
        }

        process_token
    }

    /// Handle a compute request.
    ///
    /// Returns the execution time.
    #[cfg(target_os = "linux")]
    fn handle_compute(
        lock: Arc<Mutex<()>>,
        path_manager: Arc<PathManager>,
        module_id: ModuleID,
        cached: bool,
        cold_cache_enabled: bool,
        package: Vec<u8>,
        binary_path: PathBuf,
        args: Vec<String>,
        envs: Vec<String>,
    ) -> Result<Duration, Error> {
        let now = Instant::now();
        if cached && !cold_cache_enabled {
            return Err(Error::CacheError("caching is disabled".to_string()));
        }
        // TODO: lock on finer granularity, just the specific module
        // But gets a lock around the filesystem so multiple people
        // aren't handling compute requests that could be cached.
        // And so that each request can create a directory for its process.
        let (mount, paths) = {
            let lock = lock.lock().unwrap();
            debug!("cached={} cold_cache_enabled={}", cached, cold_cache_enabled);
            if cached && !path_manager.is_cached(&module_id) {
                // TODO: controller needs to handle this error
                // what if a second request gets here before the first
                // request caches the module? race condition
                return Err(Error::CacheError(format!("module {} is not cached", module_id)));
            }
            if !cached {
                path_manager.cache_module(&module_id, package)?;
            }
            debug!("unpacked request => {} s", now.elapsed().as_secs_f32());
            let now = Instant::now();
            let (mount, paths) = path_manager.new_request(&module_id)?;
            // info!("=> preprocessing: {} s", now.elapsed().as_secs_f32());
            debug!("mounting overlayfs => {} s", now.elapsed().as_secs_f32());
            drop(lock);
            (mount, paths)
        };

        let start = Instant::now();
        let _res = runtime::run(
            &paths.root_path,
            binary_path,
            args,
            envs,
        )?;
        let execution_time = start.elapsed();
        debug!("invoked binary => {} s", execution_time.as_secs_f32());
        trace!("HANDLE_COMPUTE FINISH {}", module_id);

        // Reset the root for the next computation.
        path_manager.unmount(mount)?;
        if let Err(e) = std::fs::remove_dir_all(&paths.request_path) {
            error!("error resetting request path: {:?}", e);
        }
        let now = Instant::now();
        trace!(
            "reset directory at {:?} => {} s",
            &paths.request_path,
            now.elapsed().as_secs_f32(),
        );
        Ok(execution_time)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .filter_module("karl_host", log::LevelFilter::Debug)
        .init();
    let pwd = std::fs::canonicalize(".")?;
    let path = format!("{}/.host", std::env::var("KARL_PATH")
        .unwrap_or(pwd.as_os_str().to_str().unwrap().to_string()));
    let matches = App::new("Karl Host")
        .arg(Arg::with_name("path")
            .help("Absolute path to the host's base directory. \
                Caches modules at `<path>/cache/<module_id>`. \
                If there are multiple hosts on the same computer, they share \
                the same cache. But each host has an automatically generated \
                directory `<path>/host-<id>` for processing requests that is \
                removed when the host is killed. Each request has a root \
                directory at `<path>/host-<id>/<request>`.")
            .long("path")
            .takes_value(true)
            .default_value(&path))
        .arg(Arg::with_name("port")
            .help("Port.")
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
        .arg(Arg::with_name("cold-cache")
            .help("Whether the cold cache is enabled (0 or 1)")
            .long("cold-cache")
            .takes_value(true)
            .default_value("1"))
        .arg(Arg::with_name("warm-cache")
            .help("Whether the warm cache is enabled (0 or 1)")
            .long("warm-cache")
            .takes_value(true)
            .default_value("1"))
        .arg(Arg::with_name("pubsub")
            .help("Whether pubsub optimization is enabled (0 or 1)")
            .long("pubsub")
            .takes_value(true)
            .default_value("1"))
        .arg(Arg::with_name("no-mock-network")
            .help("If the flag is included, uses the real network.")
            .long("no-mock-network"))
        .get_matches();

    let base_path = Path::new(matches.value_of("path").unwrap()).to_path_buf();
    let port: u16 = matches.value_of("port").unwrap().parse().unwrap();
    let controller = format!(
        "http://{}:{}",
        matches.value_of("controller-ip").unwrap(),
        matches.value_of("controller-port").unwrap(),
    );
    let password = matches.value_of("password").unwrap();
    let cold_cache_enabled = matches.value_of("cold-cache").unwrap() == "1";
    let warm_cache_enabled = matches.value_of("warm-cache").unwrap() == "1";
    let pubsub_enabled = matches.value_of("pubsub").unwrap() == "1";
    let mock_network = !matches.is_present("no-mock-network");
    let mut host = Host::new(
        base_path,
        &controller,
        cold_cache_enabled,
        warm_cache_enabled,
        pubsub_enabled,
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
