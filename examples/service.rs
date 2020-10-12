#[macro_use]
extern crate log;

use std::fs;
use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Instant;

use bincode;
use dirs;
use clap::{Arg, App};
use tokio::runtime::Runtime;
use wasmer::executor::PkgConfig;
use flate2::read::GzDecoder;
use tar::Archive;

use karl::{
    self, Error, import::Import, backend::Backend,
    KarlRequest, KarlResult,
    ComputeRequest, ComputeResult, PingRequest, PingResult,
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
}

/// Read from the KARL_PATH environment variable. (TODO)
///
/// If not set, defaults to ~/.karl/.
fn get_karl_path() -> PathBuf {
    let home_dir = dirs::home_dir().unwrap();
    home_dir.join(".karl")
}

/// Unpackage the bytes of the tarred and gzipped request to the base path.
///
/// A properly formatted request will produce a `config` file and the `root`
/// compute directory at the base path. Returns the deserialized config file
/// and the root path.
///
/// TODO: Avoid this extra serialization step.
fn unpack_request(req: &ComputeRequest, base: &Path) -> (PkgConfig, PathBuf) {
    let now = Instant::now();
    std::fs::create_dir_all(base).unwrap();
    let tar = GzDecoder::new(&req.package[..]);
    let mut archive = Archive::new(tar);
    archive.unpack(base).expect(&format!("malformed tar.gz in request"));
    info!("=> unpacked request to {:?}: {} s", base, now.elapsed().as_secs_f32());

    // Deserialize the config file.
    let root_path = base.join("root");
    let config_path = base.join("config");
    let config = {
        let mut buffer = vec![];
        let mut f = fs::File::open(&config_path).expect(
            &format!("malformed package: no config file at {:?}", config_path));
        f.read_to_end(&mut buffer).expect("error reading config");
        bincode::deserialize(&buffer).expect("malformed config file")
    };
    (config, root_path)
}

/// Resolve imports.
fn resolve_import_paths(
    karl_path: &Path,
    imports: &Vec<Import>,
) -> Result<Vec<PathBuf>, Error> {
    let mut import_paths = vec![];
    for import in imports {
        let path = import.install_if_missing(karl_path)?;
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
    config: &PkgConfig,
    pkg_root: &Path,
    import_paths: &Vec<PathBuf>,
) -> Result<PathBuf, Error> {
    assert!(pkg_root.is_absolute());
    // 1.
    let bin_path = config.binary_path.as_ref().ok_or(
        Error::BinaryNotFound("binary path did not exist in config".to_string()))?;
    let path = pkg_root.join(&bin_path);
    if path.exists() {
        return Ok(path);
    }
    // 2.
    let wasm_filename = bin_path.file_name().ok_or(
        Error::BinaryNotFound(format!("malformed: {:?}", bin_path)))?;
    for import_path in import_paths {
        assert!(import_path.is_absolute());
        let path = import_path.join("bin").join(&wasm_filename);
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
    fn new(backend: Backend, port: u16) -> Self {
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
        }
    }

    /// Process an incoming connection
    fn start(&mut self) -> Result<(), Error> {
        let listener = TcpListener::bind(format!("0.0.0.0:{}", self.port))?;
        self.port = listener.local_addr()?.port();
        info!("ID {} listening on port {}", self.id, self.port);
        karl::net::register(&mut self.rt, self.id, self.port);
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
        PingResult::new()
    }

    /// Handle a compute request.
    fn handle_compute(&mut self, req: ComputeRequest) -> Result<ComputeResult, Error> {
        info!("handling compute: (len {}) stdout={} stderr={} {:?}",
            req.package.len(), req.stdout, req.stderr, req.files);
        let now = Instant::now();
        let (mut config, root_path) = unpack_request(&req, &self.base_path);
        let import_paths = resolve_import_paths(&self.karl_path, &req.imports)?;
        config.binary_path = Some(resolve_binary_path(
            &config, &root_path, &import_paths)?);
        config.mapped_dirs = get_mapped_dirs(import_paths);
        info!("=> preprocessing: {} s", now.elapsed().as_secs_f32());

        let res = match self.backend {
            Backend::Wasm => karl::backend::wasm::run(
                config,
                &root_path,
                req.stdout,
                req.stderr,
                req.files,
            )?,
            Backend::Binary => karl::backend::binary::run(
                config,
                &root_path,
                req.stdout,
                req.stderr,
                req.files,
            )?,
        };

        // Reset the root for the next computation.
        let now = Instant::now();
        std::env::set_current_dir(&self.karl_path).unwrap();
        std::fs::remove_dir_all(&self.base_path).unwrap();
        info!("reset directory at {:?} => {} s", self.base_path, now.elapsed().as_secs_f32());
        Ok(res)
    }

    /// Handle an incoming TCP stream.
    fn handle_client(&mut self, mut stream: TcpStream) -> Result<(), Error> {
        // Read the computation request from the TCP stream.
        let now = Instant::now();
        debug!("reading packet");
        let buf = karl::read_packets(&mut stream, 1)?.remove(0);
        debug!("=> {} s ({} bytes)", now.elapsed().as_secs_f32(), buf.len());

        // Deserialize the request.
        debug!("deserialize packet");
        let now = Instant::now();
        let req_bytes = bincode::deserialize(&buf[..])
            .map_err(|e| Error::SerializationError(format!("{:?}", e)))?;
        debug!("=> {} s", now.elapsed().as_secs_f32());

        // Deploy the request to correct handler.
        let res = match req_bytes {
            KarlRequest::Ping(req) => KarlResult::Ping(self.handle_ping(req)),
            KarlRequest::Compute(req) => KarlResult::Compute(self.handle_compute(req)?),
        };

        // Return the result to sender.
        debug!("serialize packet");
        debug!("=> {:?}", res);
        let now = Instant::now();
        let res_bytes = bincode::serialize(&res)
            .map_err(|e| Error::SerializationError(format!("{:?}", e)))?;
        debug!("=> {} s", now.elapsed().as_secs_f32());

        debug!("writing packet");
        let now = Instant::now();
        karl::write_packet(&mut stream, &res_bytes)?;
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
        .get_matches();

    let backend = match matches.value_of("backend").unwrap() {
        "wasm" => Backend::Wasm,
        "binary" => Backend::Binary,
        backend => unimplemented!("unimplemented backend: {}", backend),
    };
    let mut listener = Listener::new(backend, 0);
    listener.start().unwrap();
}
