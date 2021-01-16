use std::fs;
use std::thread;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};

use tokio::runtime::Runtime;
use flate2::read::GzDecoder;
use tar::Archive;

use protobuf;
use protobuf::Message;
use crate::{packet, protos, backend::Backend};
use crate::common::{
    Error, Token, RequestToken,
    HT_COMPUTE_REQUEST, HT_COMPUTE_RESULT, HT_PING_REQUEST, HT_PING_RESULT,
};

/// Frequency at which the host must send messages to the controller, in seconds.
pub const HEARTBEAT_INTERVAL: u64 = 10;

pub struct Host {
    /// Node/service ID
    id: u32,
    /// Karl path, likely ~/.karl
    karl_path: PathBuf,
    /// Computation request base, likely ~/.karl/<id>
    /// Computation root likely at ~/.karl/<id>/root/
    base_path: PathBuf,
    backend: Backend,
    port: u16,
    rt: Runtime,
    /// Controller address.
    controller: String,
    /// Request token and when it was last updated.
    ///
    /// The host will only accept a ComputeRequest if it includes the
    /// active RequestToken. The token is set to None while the host is
    /// is processing a ComputeRequest.
    token: Arc<Mutex<(Option<RequestToken>, Instant)>>,
}

/// Unpackage the bytes of the tarred and gzipped request to the base path.
/// This is the input root which will be overlayed on top of any imports.
///
/// Creates the root path directory if it does not already exist.
fn unpack_request(req: &protos::ComputeRequest, root: &Path) -> Result<(), Error> {
    let now = Instant::now();
    let tar = GzDecoder::new(&req.get_package()[..]);
    let mut archive = Archive::new(tar);
    fs::create_dir_all(root).unwrap();
    archive.unpack(root).map_err(|e| format!("malformed tar.gz: {:?}", e))?;
    info!("=> unpacked request to {:?}: {} s", root, now.elapsed().as_secs_f32());
    Ok(())
}

/// Resolve imports.
fn resolve_import_paths(
    karl_path: &Path,
    imports: &Vec<protos::Import>,
) -> Result<Vec<PathBuf>, Error> {
    let mut import_paths = vec![];
    for import in imports {
        let path = crate::common::import_path(&import, karl_path);
        import_paths.push(path);
    }
    Ok(import_paths)
}

/// Get mapped directories.
///
/// Maps imports to the package root.
fn get_mapped_dirs(import_paths: Vec<PathBuf>) -> Vec<String> {
    import_paths
        .into_iter()
        .map(|path| path.into_os_string().into_string().unwrap())
        .map(|path| format!(".:{}", path))
        .collect()
}

/// Resolve the actual host binary path based on the config binary path.
///
/// Find an existing path in the following order:
/// 1. Relative to the package root.
/// 2. Relative to import paths.
/// 3. Otherwise, errors with Error::BinaryNotFound.
fn resolve_binary_path(
    config: &protos::PkgConfig,
    pkg_root: &Path,
    import_paths: &Vec<PathBuf>,
) -> Result<PathBuf, Error> {
    assert!(pkg_root.is_absolute());
    // 1.
    let bin_path = Path::new(config.get_binary_path());
    let path = pkg_root.join(&bin_path);
    if path.exists() {
        return Ok(path);
    }
    // 2.
    let filename = bin_path.file_name().ok_or(
        Error::BinaryNotFound(format!("malformed: {:?}", bin_path)))?;
    for import_path in import_paths {
        assert!(import_path.is_absolute());
        let path = import_path.join("bin").join(&filename);
        if path.exists() {
            return Ok(path);
        }
        let path = import_path.join(&bin_path);
        if path.exists() {
            return Ok(path);
        }
    }
    // 3.
    Err(Error::BinaryNotFound(format!("not found: {:?}", bin_path)))
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base_path);
    }
}

impl Host {
    /// Generate a new listener with a random ID.
    ///
    /// The constructor creates a directory at the <KARL_PATH> if it does
    /// not already exist. The working directory for any computation is at
    /// <KARL_PATH>/<LISTENER_ID>. When not doing computation, the working
    /// directory must be at <KARL_PATH>.
    ///
    /// Note that the wasmer runtime changes the working directory for
    /// computation, so the listener must change it back immediately after.
    pub fn new(
        karl_path: PathBuf,
        backend: Backend,
        port: u16,
        controller: &str,
    ) -> Self {
        use rand::Rng;
        let id: u32 = rand::thread_rng().gen();
        let base_path = karl_path.join(id.to_string());

        // Create the <KARL_PATH> if it does not already exist.
        fs::create_dir_all(&karl_path).unwrap();
        // Set the current working directory to the <KARL_PATH>.
        std::env::set_current_dir(&karl_path).unwrap();
        debug!("create karl_path {:?}", &karl_path);
        Self {
            id,
            karl_path,
            base_path,
            backend,
            port,
            rt: Runtime::new().unwrap(),
            controller: controller.to_string(),
            token: Arc::new(Mutex::new((None, Instant::now()))),
        }
    }

    /// Spawns a background process that sends heartbeats to the controller
    /// at the HEARTBEAT_INTERVAL and processes incoming connections.
    ///
    /// Parameters:
    /// - register - Whether the listener should register itself on DNS-SD.
    pub fn start(&mut self, register: bool) -> Result<(), Error> {
        let listener = TcpListener::bind(format!("0.0.0.0:{}", self.port))?;
        self.port = listener.local_addr()?.port();
        info!("ID {} listening on port {}", self.id, self.port);

        if register {
            crate::net::register(&mut self.rt, self.id, self.port);
        } else {
            warn!("you must manually register the service on DNS-SD!")
        }

        let token_lock = self.token.clone();
        let controller_addr = self.controller.clone();
        let host_id = self.id.clone();
        self.rt.spawn(async move {
            // Send the initial heartbeat.
            let mut token = token_lock.lock().unwrap();
            let token_to_send = Token::gen();
            *token = (Some(token_to_send.clone()), Instant::now());
            debug!("heartbeat {:?}", &token_to_send);
            crate::net::heartbeat(&controller_addr, host_id, token_to_send);
            drop(token);

            // Every HEARTBEAT_INTERVAL seconds, this process wakes up
            // and determine if it needs to send a heartbeat message with
            // a new request token. Note request tokens are also sent with
            // NotifyEnd messages.
            //
            // If the last message the host sent is a NotifyStart, and thus
            // the host is processing an active request, it does not need to
            // send a heartbeat. Otherwise, if the last message is a NotifyEnd
            // or a Heartbeat, and more than HEARTBEAT_INTERVAL seconds has
            // elapsed, the process will send a heartbeat.
            //
            // Note that the host may generate a new request token in the time
            // that the controller allocates a host token and the client sends
            // a ComputeRequest. In that case, the client must retry the
            // HostRequest with the controller (equivalent to client timeout).
            // This mechanism protects the host from clients that reserve
            // hosts with the controller and fails to make a ComputeRequest.
            loop {
                thread::sleep(Duration::from_secs(HEARTBEAT_INTERVAL));
                let mut token = token_lock.lock().unwrap();
                if token.0.is_some() && token.1.elapsed().as_secs_f32() > HEARTBEAT_INTERVAL as f32 {
                    let token_to_send = Token::gen();
                    *token = (Some(token_to_send.clone()), Instant::now());
                    debug!("heartbeat {:?}", &token_to_send);
                    crate::net::heartbeat(&controller_addr, host_id, token_to_send);
                }
                drop(token);
            }
        });

        for stream in listener.incoming() {
            let stream = stream?;
            debug!("incoming stream {:?}", stream.peer_addr());
            let now = Instant::now();
            if let Err(e) = self.handle_client(stream) {
                error!("{:?}", e);
            }
            warn!("total: {} s", now.elapsed().as_secs_f32());
        }
        Ok(())
    }

    /// Handle a ping request.
    fn handle_ping(&mut self, _req: protos::PingRequest) -> protos::PingResult {
        protos::PingResult::default()
    }

    /// Handle a compute request.
    ///
    /// Verifies that the compute request includes a valid request token.
    /// Notifies the controller of the start and end of the request, if
    /// the request token is valid and the host processes compute.
    fn handle_compute(
        &mut self,
        req: protos::ComputeRequest,
    ) -> Result<protos::ComputeResult, Error> {
        info!("handling compute from {:?}: (len {}) stdout={} stderr={} storage={} {:?}",
            req.client_id, req.package.len(), req.stdout, req.stderr, req.storage, req.files);
        let now = Instant::now();

        // Invalidate the token for future requests and notify start.
        let mut token = self.token.lock().unwrap();
        *token = (None, Instant::now());
        drop(token);
        crate::net::notify_start(&self.controller, self.id, req.client_id.clone());

        let root_path = self.base_path.join("root");
        unpack_request(&req, &root_path)?;
        let import_paths = resolve_import_paths(
            &self.karl_path, &req.imports.to_vec())?;
        let binary_path = resolve_binary_path(
            req.get_config(), &root_path, &import_paths)?;
        let mapped_dirs = get_mapped_dirs(import_paths);
        info!("=> preprocessing: {} s", now.elapsed().as_secs_f32());

        let res = match self.backend {
            #[cfg(feature = "wasm")]
            Backend::Wasm => crate::backend::wasm::run(
                binary_path,
                mapped_dirs,
                req.get_config().get_args().to_vec(),
                req.get_config().get_envs().to_vec(),
                &root_path,
                req.stdout,
                req.stderr,
                req.files.to_vec().into_iter().collect(),
            )?,
            #[cfg(not(feature = "wasm"))]
            Backend::Wasm => unreachable!(),
            Backend::Binary => crate::backend::binary::run(
                binary_path,
                mapped_dirs,
                req.get_config().get_args().to_vec(),
                req.get_config().get_envs().to_vec(),
                &self.karl_path,
                &self.base_path,
                req.get_client_id(),
                req.get_storage(),
                req.stdout,
                req.stderr,
                req.files.to_vec().into_iter().collect(),
            )?,
        };

        // Reset the root for the next computation.
        std::env::set_current_dir(&self.karl_path).unwrap();
        if let Err(e) = std::fs::remove_dir_all(&self.base_path) {
            error!("error resetting root: {:?}", e);
        }
        let now = Instant::now();
        info!(
            "reset directory at {:?} => {} s",
            self.base_path,
            now.elapsed().as_secs_f32(),
        );

        // Notify end and create a new token.
        let mut token = self.token.lock().unwrap();
        *token = (Some(Token::gen()), Instant::now());
        crate::net::notify_end(&self.controller, self.id, token.0.clone().unwrap());
        Ok(res)
    }

    /// Handle an incoming TCP stream.
    fn handle_client(&mut self, mut stream: TcpStream) -> Result<(), Error> {
        // Read the computation request from the TCP stream.
        let now = Instant::now();
        debug!("reading packet");
        let (header, buf) = packet::read(&mut stream, 1)?.remove(0);
        debug!("=> {} s ({} bytes)", now.elapsed().as_secs_f32(), buf.len());

        // Deploy the request to correct handler.
        let (res_bytes, ty) = match header.ty {
            HT_PING_REQUEST => {
                debug!("deserialize packet");
                let now = Instant::now();
                let req = protobuf::parse_from_bytes::<protos::PingRequest>(&buf[..])
                    .map_err(|e| Error::SerializationError(format!("{:?}", e)))?;
                debug!("=> {} s", now.elapsed().as_secs_f32());
                let res = self.handle_ping(req);
                debug!("=> {:?}", res);
                debug!("serialize packet");
                let now = Instant::now();
                let res_bytes = res.write_to_bytes()
                    .map_err(|e| Error::SerializationError(format!("{:?}", e)))?;
                debug!("=> {} s", now.elapsed().as_secs_f32());
                (res_bytes, HT_PING_RESULT)
            },
            HT_COMPUTE_REQUEST => {
                debug!("deserialize packet");
                let now = Instant::now();
                let req = protobuf::parse_from_bytes::<protos::ComputeRequest>(&buf[..])
                    .map_err(|e| Error::SerializationError(format!("{:?}", e)))?;
                debug!("=> {} s", now.elapsed().as_secs_f32());
                let res = self.handle_compute(req)?;
                debug!("=> {:?}", res);
                debug!("serialize packet");
                let now = Instant::now();
                let res_bytes = res.write_to_bytes()
                    .map_err(|e| Error::SerializationError(format!("{:?}", e)))?;
                debug!("=> {} s", now.elapsed().as_secs_f32());
                (res_bytes, HT_COMPUTE_RESULT)
            },
            ty => return Err(Error::InvalidPacketType(ty)),
        };

        // Return the result to sender.
        debug!("writing packet");
        let now = Instant::now();
        packet::write(&mut stream, ty, &res_bytes)?;
        debug!("=> {} s", now.elapsed().as_secs_f32());
        Ok(())
    }
}

#[cfg(test)]
mod test {

}
