#[macro_use]
extern crate log;

use std::fs;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Instant;

use clap::{Arg, App};
use tokio::runtime::Runtime;
use flate2::read::GzDecoder;
use tar::Archive;

use protobuf;
use protobuf::Message;
use karl::{self, packet, backend::Backend};
use karl::protos::{
    Import, ComputeRequest, ComputeResult, PingRequest, PingResult,
};
use karl::common::{
    Error, PkgConfig,
    HT_COMPUTE_REQUEST, HT_COMPUTE_RESULT, HT_PING_REQUEST, HT_PING_RESULT,
};

struct Listener {
    /// Node/service ID
    id: u32,
    /// Karl path, likely ~/.karl
    karl_path: PathBuf,
    /// Computation request base, likely ~/.karl/<id>
    /// Config likely at ~/.karl/<id>/config
    /// Computation root likely at ~/.karl/<id>/root/
    base_path: PathBuf,
    backend: Backend,
    port: u16,
    rt: Runtime,
    /// Whether the listener should register itself on DNS-SD
    register: bool,
}

/// Read from the KARL_PATH environment variable. (TODO)
/// If not set, defaults to ~/.karl/.
///
/// ACTUALLY, hardcoded to "/home/gina/.karl" since homedir can't
/// resolve correctly when running with root, and root is needed to mount.
fn get_karl_path() -> PathBuf {
    Path::new("/home/gina/.karl").to_path_buf()
}

/// Unpackage the bytes of the tarred and gzipped request to the base path.
/// This is the input root which will be overlayed on top of any imports.
///
/// Creates the root path directory if it does not already exist.
fn unpack_request(req: &ComputeRequest, root: &Path) -> Result<(), Error> {
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
    imports: &Vec<Import>,
) -> Result<Vec<PathBuf>, Error> {
    let mut import_paths = vec![];
    for import in imports {
        let path = karl::common::import_path(&import, karl_path);
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
    config: &karl::protos::PkgConfig,
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

impl Drop for Listener {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base_path);
    }
}

impl Listener {
    /// Generate a new listener with a random ID.
    ///
    /// The KARL_PATH defaults to ~/.karl. The constructor creates a directory
    /// at the <KARL_PATH> if it does not already exist. The working directory
    /// for any computation is at <KARL_PATH>/<LISTENER_ID>. When not doing
    /// computation, the working directory must be at <KARL_PATH>.
    ///
    /// Note that the wasmer runtime changes the working directory for
    /// computation, so the listener must change it back immediately after.
    fn new(backend: Backend, port: u16, register: bool) -> Self {
        use rand::Rng;
        let id: u32 = rand::thread_rng().gen();
        let karl_path = get_karl_path();
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
            register,
        }
    }

    /// Process an incoming connection
    fn start(&mut self) -> Result<(), Error> {
        let listener = TcpListener::bind(format!("0.0.0.0:{}", self.port))?;
        self.port = listener.local_addr()?.port();
        info!("ID {} listening on port {}", self.id, self.port);
        if self.register {
            karl::net::register(&mut self.rt, self.id, self.port);
        } else {
            warn!("you must manually register the service on DNS-SD!")
        }
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

    /// Handle a ping request.
    fn handle_ping(&mut self, _req: PingRequest) -> PingResult {
        PingResult::default()
    }

    /// Handle a compute request.
    fn handle_compute(
        &mut self,
        req: ComputeRequest,
    ) -> Result<ComputeResult, Error> {
        info!("handling compute from {:?}: (len {}) stdout={} stderr={} storage={} {:?}",
            req.client_id, req.package.len(), req.stdout, req.stderr, req.storage, req.files);
        let now = Instant::now();
        let root_path = self.base_path.join("root");
        unpack_request(&req, &root_path)?;
        let import_paths = resolve_import_paths(
            &self.karl_path, &req.imports.to_vec())?;
        let binary_path = Some(resolve_binary_path(
            req.get_config(), &root_path, &import_paths)?);
        let mapped_dirs = get_mapped_dirs(import_paths);
        let config = PkgConfig {
            binary_path,
            mapped_dirs,
            args: req.get_config().get_args().to_vec(),
            envs: req.get_config().get_envs().to_vec(),
        };
        info!("=> preprocessing: {} s", now.elapsed().as_secs_f32());

        let res = match self.backend {
            Backend::Wasm => karl::backend::wasm::run(
                config,
                &root_path,
                req.stdout,
                req.stderr,
                req.files.to_vec().into_iter().collect(),
            )?,
            Backend::Binary => karl::backend::binary::run(
                config,
                &self.base_path,
                req.stdout,
                req.stderr,
                req.files.to_vec().into_iter().collect(),
            )?,
        };

        // Reset the root for the next computation.
        let now = Instant::now();
        std::env::set_current_dir(&self.karl_path).unwrap();
        if let Err(e) = std::fs::remove_dir_all(&self.base_path) {
            error!("error resetting root: {:?}", e);
        }
        info!(
            "reset directory at {:?} => {} s",
            self.base_path,
            now.elapsed().as_secs_f32(),
        );
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
                let req = protobuf::parse_from_bytes::<PingRequest>(&buf[..])
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
                let req = protobuf::parse_from_bytes::<ComputeRequest>(&buf[..])
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

fn main() {
    env_logger::builder().format_timestamp(None).init();
    let matches = App::new("Karl Service")
        .arg(Arg::with_name("backend")
            .help("Service backend. Either 'wasm' for wasm executables or \
                `binary` for binary executables. Assumes macOS executables \
                only.")
            .short("b")
            .long("backend")
            .takes_value(true)
            .default_value("wasm"))
        .arg(Arg::with_name("port")
            .help("Port. Defaults to a random open port.")
            .short("p")
            .long("port")
            .takes_value(true)
            .default_value("0"))
        .arg(Arg::with_name("no-register")
            .help("If the flag is included, does not automatically register \
                the service with DNS-SD. The default is to register.")
            .long("no-register"))
        .get_matches();

    let backend = match matches.value_of("backend").unwrap() {
        "wasm" => Backend::Wasm,
        "binary" => Backend::Binary,
        backend => unimplemented!("unimplemented backend: {}", backend),
    };
    let port: u16 = matches.value_of("port").unwrap().parse().unwrap();
    let register = !matches.is_present("no-register");
    let mut listener = Listener::new(backend, port, register);
    listener.start().unwrap();
}
