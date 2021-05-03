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
    /// Whether to validate tokens.
    validate: bool,
}

#[tonic::async_trait]
impl karl_host_server::KarlHost for Host {
    async fn start_compute(
        &self, req: Request<ComputeRequest>,
    ) -> Result<Response<NotifyStart>, Status> {
        let now = Instant::now();
        warn!("step 5: host receives compute request");
        let mut req = req.into_inner();
        let process_token = Token::gen();
        let perms = ProcessPerms::new(&req);

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
            tokio::spawn(async move {
                req.envs.push(format!("PROCESS_TOKEN={}", &process_token));
                warn!("=> {} s", now.elapsed().as_secs_f32());
                Host::handle_compute(
                    path_manager,
                    req.hook_id,
                    req.cached,
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
                return Err(Status::new(Code::Unauthenticated, "invalid ACL"));
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

    async fn get(
        &self, req: Request<GetData>,
    ) -> Result<Response<GetDataResult>, Status> {
        // Validate the process is valid and has permissions to read the file.
        // No serializability guarantees from other requests from the same process.
        // Sanitizes the path.
        let req = req.into_inner();
        debug!("get {:?}", req);
        if self.validate {
            if let Some(_perms) = self.process_tokens.lock().unwrap().get(&req.process_token) {
                // if !perms.can_read_file(Path::new(&req.path)) {
                //     return Err(Status::new(Code::Unauthenticated, "invalid ACL"));
                // }
            } else {
                return Err(Status::new(Code::Unauthenticated, "invalid process token"));
            }
        }

        // Forward the file access to the controller and return the result
        self.api.forward_get(req).await
    }

    async fn push(
        &self, req: Request<PushData>,
    ) -> Result<Response<()>, Status> {
        // Validate the process is valid and has permissions to write the file.
        // No serializability guarantees from other requests from the same process.
        // Sanitizes the path.
        let req = req.into_inner();
        let sensor_key = if let Some(perms) = self.process_tokens.lock().unwrap().get(&req.process_token) {
            // if !perms.can_write_file(Path::new(&req.path)) {
            //     return Err(Status::new(Code::Unauthenticated, "invalid ACL"));
            // }
            if let Some(sensor_key) = perms.can_change_state(&req.tag) {
                let mut split = sensor_key.split(".");
                let sensor = split.next().unwrap();
                let key = split.next().unwrap();
                Some((sensor.to_string(), key.to_string()))
            } else {
                None
            }
        } else {
            return Err(Status::new(Code::Unauthenticated, "invalid process token"));
        };

        if let Some((sensor, key)) = sensor_key {
            // Forward as state change if the tag changes state.
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
            self.api.forward_push(req).await
        }
    }
}

impl Host {
    /// Generate a new host with a random ID.
    pub fn new(
        karl_path: PathBuf,
        controller: &str,
    ) -> Self {
        use rand::Rng;
        let id: u32 = rand::thread_rng().gen();
        Self {
            id,
            api: crate::net::KarlHostAPI::new(controller),
            process_tokens: Arc::new(Mutex::new(HashMap::new())),
            path_manager: Arc::new(PathManager::new(karl_path, id)),
            validate: false,
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
        path_manager: Arc<PathManager>,
        hook_id: HookID,
        cached: bool,
        package: Vec<u8>,
        binary_path: PathBuf,
        args: Vec<String>,
        envs: Vec<String>,
    ) -> Result<(), Error> {
        info!("handling compute (len {}) cached={}: command => {:?} {:?} envs => {:?}",
            cached, package.len(), binary_path, args, envs);

        let now = Instant::now();
        warn!("step 6a: unpacking request");
        if cached && !path_manager.is_cached(&hook_id) {
            return Err(Error::CacheError(format!("hook {} is not cached", hook_id)));
        }
        if !cached {
            path_manager.cache_hook(&hook_id, package)?;
        }
        warn!("=> {} s", now.elapsed().as_secs_f32());
        warn!("step 6b: mounting request overlayfs");
        let (mount, paths) = path_manager.new_request(&hook_id);
        // info!("=> preprocessing: {} s", now.elapsed().as_secs_f32());
        warn!("=> {} s", now.elapsed().as_secs_f32());

        let now = Instant::now();
        warn!("step 7: invoke the binary");
        let _res = runtime::run(
            &paths.root_path,
            binary_path,
            args,
            envs,
        )?;
        warn!("=> {} s", now.elapsed().as_secs_f32());

        // Reset the root for the next computation.
        path_manager.unmount(mount);
        if let Err(e) = std::fs::remove_dir_all(&paths.request_path) {
            error!("error resetting request path: {:?}", e);
        }
        let now = Instant::now();
        info!(
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
        .get_matches();

    let karl_path = Path::new(matches.value_of("karl-path").unwrap()).to_path_buf();
    let port: u16 = matches.value_of("port").unwrap().parse().unwrap();
    let controller = format!(
        "http://{}:{}",
        matches.value_of("controller-ip").unwrap(),
        matches.value_of("controller-port").unwrap(),
    );
    let password = matches.value_of("password").unwrap();
    let mut host = Host::new(karl_path, &controller);
    host.start(port, password).await.unwrap();
    Server::builder()
        .add_service(KarlHostServer::new(host))
        .serve(format!("0.0.0.0:{}", port).parse()?)
        .await
        .unwrap();
    Ok(())
}
