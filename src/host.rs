use std::fs;
use std::collections::{HashSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};

use tokio;
use reqwest::{self, Method, header::HeaderName};
use flate2::read::GzDecoder;
use tar::Archive;

use tonic::{Request, Response, Status, Code};
use crate::protos::karl_controller_client::KarlControllerClient;
use crate::protos::*;
use crate::common::*;

/// Frequency at which the host must send messages to the controller, in seconds.
pub const HEARTBEAT_INTERVAL: u64 = 10;

/// Permissions of an active process
struct ProcessPerms {
    /// State change permissions
    state_perms: HashSet<SensorID>,
    /// Network permissions
    network_perms: HashSet<String>,
    /// File read permissions
    read_perms: HashSet<PathBuf>,
    /// File write permissions
    write_perms: HashSet<PathBuf>,
}

impl ProcessPerms {
    pub fn new(req: &ComputeRequest) -> Self {
        let state_perms: HashSet<_> = req.state_perm.clone().into_iter().collect();
        let network_perms: HashSet<_> = req.network_perm.clone().into_iter().collect();
        let read_perms: HashSet<_> = req.file_perm.iter()
            .filter(|acl| acl.read)
            .map(|acl| Path::new(&sanitize_path(&acl.path)).to_path_buf())
            .collect();
        // Processes cannot write to raw/
        let write_perms: HashSet<_> = req.file_perm.iter()
            .filter(|acl| acl.write)
            .map(|acl| sanitize_path(&acl.path))
            .filter(|path| !(path == "raw" || path.starts_with("raw/")))
            .map(|path| Path::new(&path).to_path_buf())
            .collect();
        Self { state_perms, network_perms, read_perms, write_perms }
    }

    pub fn can_change_state(&self, sensor_id: &SensorID) -> bool {
        self.state_perms.contains(sensor_id)
    }

    pub fn can_access_domain(&self, domain: &str) -> bool {
        self.network_perms.contains(domain)
    }

    pub fn can_read_file(&self, path: &Path) -> bool {
        let mut next_path = Some(path);
        while let Some(path) = next_path {
            if self.read_perms.contains(path) {
                return true;
            }
            next_path = path.parent();
        }
        false
    }

    pub fn can_write_file(&self, path: &Path) -> bool {
        let mut next_path = Some(path);
        while let Some(path) = next_path {
            if self.write_perms.contains(path) {
                return true;
            }
            next_path = path.parent();
        }
        false
    }
}

pub struct Host {
    /// Host ID (unique among hosts)
    id: u32,
    /// Secret host token, assigned after registering with controller
    host_token: Option<HostToken>,
    /// Karl path, likely ~/.karl
    karl_path: PathBuf,
    /// Computation request base, likely ~/.karl/<id>
    /// Computation root likely at ~/.karl/<id>/root/
    base_path: PathBuf,
    /// Controller address.
    controller: String,
    /// Active process tokens.
    process_tokens: Arc<Mutex<HashMap<ProcessToken, ProcessPerms>>>,
}

#[tonic::async_trait]
impl karl_host_server::KarlHost for Host {
    async fn start_compute(
        &self, req: Request<ComputeRequest>,
    ) -> Result<Response<NotifyStart>, Status> {
        let req = req.into_inner();
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
            let karl_path = self.karl_path.clone();
            let base_path = self.base_path.clone();
            let controller = self.controller.clone();
            let process_tokens = self.process_tokens.clone();
            let host_token = self.host_token.clone().ok_or(
                Status::new(Code::Unavailable, "host is not registered with controller"))?;
            let process_token = process_token.clone();
            tokio::spawn(async move {
                Host::handle_compute(
                    karl_path,
                    base_path,
                    req.package,
                    binary_path,
                    req.args,
                    req.envs,
                ).unwrap();
                process_tokens.lock().unwrap().remove(&process_token);
                crate::net::notify_end(&controller, host_token, process_token).await.unwrap();
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
        let mut req = req.into_inner();
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
        req.host_token = self.host_token.clone().ok_or(
            Status::new(Code::Unavailable, "host is not registered with controller"))?;
        KarlControllerClient::connect(self.controller.clone()).await
            .map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?
            .forward_network(Request::new(req)).await?;

        // Return the result of the HTTP request.
        handle.await.map_err(|e| Status::new(Code::Internal, format!("{}", e)))?
    }

    async fn get(
        &self, req: Request<GetData>,
    ) -> Result<Response<GetDataResult>, Status> {
        // Validate the process is valid and has permissions to read the file.
        // No serializability guarantees from other requests from the same process.
        // Sanitizes the path.
        let mut req = req.into_inner();
        req.path = sanitize_path(&req.path);
        if let Some(perms) = self.process_tokens.lock().unwrap().get(&req.process_token) {
            if !perms.can_read_file(Path::new(&req.path)) {
                return Err(Status::new(Code::Unauthenticated, "invalid ACL"));
            }
        } else {
            return Err(Status::new(Code::Unauthenticated, "invalid process token"));
        }

        // Forward the file access to the controller and return the result
        req.host_token = self.host_token.clone().ok_or(
            Status::new(Code::Unavailable, "host is not registered with controller"))?;
        KarlControllerClient::connect(self.controller.clone()).await
            .map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?
            .forward_get(Request::new(req)).await
    }

    async fn put(
        &self, req: Request<PutData>,
    ) -> Result<Response<()>, Status> {
        // Validate the process is valid and has permissions to write the file.
        // No serializability guarantees from other requests from the same process.
        // Sanitizes the path.
        let mut req = req.into_inner();
        req.path = sanitize_path(&req.path);
        if let Some(perms) = self.process_tokens.lock().unwrap().get(&req.process_token) {
            if !perms.can_write_file(Path::new(&req.path)) {
                return Err(Status::new(Code::Unauthenticated, "invalid ACL"));
            }
        } else {
            return Err(Status::new(Code::Unauthenticated, "invalid process token"));
        }

        // Forward the file access to the controller and return the result
        req.host_token = self.host_token.clone().ok_or(
            Status::new(Code::Unavailable, "host is not registered with controller"))?;
        KarlControllerClient::connect(self.controller.clone()).await
            .map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?
            .forward_put(Request::new(req)).await
    }

    async fn delete(
        &self, req: Request<DeleteData>,
    ) -> Result<Response<()>, Status> {
        // Validate the process is valid and has permissions to write the file.
        // No serializability guarantees from other requests from the same process.
        // Sanitizes the path.
        let mut req = req.into_inner();
        req.path = sanitize_path(&req.path);
        if let Some(perms) = self.process_tokens.lock().unwrap().get(&req.process_token) {
            if !perms.can_write_file(Path::new(&req.path)) {
                return Err(Status::new(Code::Unauthenticated, "invalid ACL"));
            }
        } else {
            return Err(Status::new(Code::Unauthenticated, "invalid process token"));
        }

        // Forward the file access to the controller and return the result
        req.host_token = self.host_token.clone().ok_or(
            Status::new(Code::Unavailable, "host is not registered with controller"))?;
        KarlControllerClient::connect(self.controller.clone()).await
            .map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?
            .forward_delete(Request::new(req)).await
    }

    async fn state(
        &self, req: Request<StateChange>,
    ) -> Result<Response<()>, Status> {
        // Validate the process is valid and has permissions to change state.
        // No serializability guarantees from other requests from the same process.
        // Sanitizes the path.
        let mut req = req.into_inner();
        if let Some(perms) = self.process_tokens.lock().unwrap().get(&req.process_token) {
            if !perms.can_change_state(&req.sensor_id) {
                return Err(Status::new(Code::Unauthenticated, "invalid ACL"));
            }
        } else {
            return Err(Status::new(Code::Unauthenticated, "invalid process token"));
        }

        // Forward the file access to the controller and return the result
        req.host_token = self.host_token.clone().ok_or(
            Status::new(Code::Unavailable, "host is not registered with controller"))?;
        KarlControllerClient::connect(self.controller.clone()).await
            .map_err(|e| Status::new(Code::Internal, format!("{:?}", e)))?
            .forward_state(Request::new(req)).await
    }
}

/// Unpackage the bytes of the tarred and gzipped request to the base path.
/// This is the input root.
///
/// Creates the root path directory if it does not already exist.
fn unpack_request(package: &[u8], root: &Path) -> Result<(), Error> {
    let now = Instant::now();
    let tar = GzDecoder::new(package);
    let mut archive = Archive::new(tar);
    fs::create_dir_all(root).unwrap();
    archive.unpack(root).map_err(|e| format!("malformed tar.gz: {:?}", e))?;
    info!("=> unpacked request to {:?}: {} s", root, now.elapsed().as_secs_f32());
    Ok(())
}

/// Sanitize a path.
///
/// Transforms into a relative path regardless of host filesystem and removes
/// dots and trailing slashes e.g. ./raw/../camera/ --> raw/camera
///
/// Actual path is /home/user/.karl_controller/data/raw/camera.
fn sanitize_path(path: &str) -> String {
    let mut new_path = Path::new("").to_path_buf();
    let components = Path::new(path)
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(path) => Some(Path::new(path)),
            _ => None,
        });
    for component in components {
        new_path = new_path.join(component);
    }
    new_path.into_os_string().into_string().unwrap()
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base_path);
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
        let base_path = karl_path.join(id.to_string());
        Self {
            id,
            host_token: None,
            karl_path,
            base_path,
            controller: controller.to_string(),
            process_tokens: Arc::new(Mutex::new(HashMap::new())),
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
        // Create the <KARL_PATH> if it does not already exist.
        fs::create_dir_all(&self.karl_path).unwrap();
        // Set the current working directory to the <KARL_PATH>.
        std::env::set_current_dir(&self.karl_path).unwrap();
        debug!("create karl_path {:?}", &self.karl_path);

        let controller_addr = self.controller.clone();
        let host_token = crate::net::register_host(
            &self.controller, self.id, port, password).await?.into_inner().host_token;
        self.host_token = Some(host_token.clone());
        tokio::spawn(async move {
            // Every HEARTBEAT_INTERVAL seconds, this process wakes up
            // sends a heartbeat message to the controller.
            loop {
                tokio::time::sleep(Duration::from_secs(HEARTBEAT_INTERVAL)).await;
                debug!("heartbeat {}", &host_token);
                let res = crate::net::heartbeat(
                    &controller_addr,
                    host_token.clone(),
                ).await;
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
        karl_path: PathBuf,
        base_path: PathBuf,
        package: Vec<u8>,
        binary_path: PathBuf,
        args: Vec<String>,
        envs: Vec<String>,
    ) -> Result<(), Error> {
        info!("handling compute (len {}): command => {:?} {:?} envs => {:?}",
            package.len(), binary_path, args, envs);
        let now = Instant::now();

        use rand::Rng;
        let random_path: u32 = rand::thread_rng().gen();
        let root_path = base_path.join(random_path.to_string());
        unpack_request(&package[..], &root_path)?;
        info!("=> preprocessing: {} s", now.elapsed().as_secs_f32());

        let _res = crate::runtime::run(
            binary_path,
            args,
            envs,
            &karl_path,
            &root_path,
        )?;

        // Reset the root for the next computation.
        std::env::set_current_dir(&karl_path).unwrap();
        if let Err(e) = std::fs::remove_dir_all(&root_path) {
            error!("error resetting root: {:?}", e);
        }
        let now = Instant::now();
        info!(
            "reset directory at {:?} => {} s",
            root_path,
            now.elapsed().as_secs_f32(),
        );
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_sanitize_path() {
        assert_eq!(&sanitize_path("raw/cam"), "raw/cam");
        assert_eq!(&sanitize_path("/raw/cam"), "raw/cam");
        assert_eq!(&sanitize_path("./raw/cam"), "raw/cam");
        assert_eq!(&sanitize_path("../raw/cam"), "raw/cam");
        assert_eq!(&sanitize_path("raw/../cam"), "raw/cam");
        assert_eq!(&sanitize_path("./raw/../cam/"), "raw/cam");
    }
}
