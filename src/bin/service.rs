#[macro_use]
extern crate log;

use std::fs;
use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Instant;

use bincode;
use dirs;
use tokio::runtime::Runtime;
use astro_dnssd::register::DNSServiceBuilder;
use wasmer::executor::{replay_with_config, Run, PkgConfig};
use flate2::read::GzDecoder;
use tar::Archive;

use karl::{import::Import, *};

struct Listener {
    /// Node/service ID
    id: u32,
    /// Karl path
    karl_path: PathBuf,
    /// Computation root
    root: PathBuf,
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

/// Write the bytes of the packaged compute request to a temporary file.
/// The file will be removed after the results are returned to the sender.
///
/// TODO: Avoid this extra serialization step.
fn unpack_request(req: &ComputeRequest, root: &Path) {
    let now = Instant::now();
    std::fs::create_dir_all(root).unwrap();
    let tar = GzDecoder::new(&req.package[..]);
    let mut archive = Archive::new(tar);
    archive.unpack(root).expect(&format!("malformed tar.gz in request"));
    info!("=> unpacked request to {:?}: {} s", root, now.elapsed().as_secs_f32());
}

/// Parse the config and root path directory given the package root.
fn parse_request(pkg_root: &Path) -> (PkgConfig, PathBuf) {
    assert!(pkg_root.is_dir());
    let root_path = pkg_root.join("root");
    let config_path = pkg_root.join("config");
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
        let _ = std::fs::remove_dir_all(&self.root);
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
    fn new(port: u16) -> Self {
        use rand::Rng;
        let id: u32 = rand::thread_rng().gen();
        let karl_path = get_karl_path();
        let root = karl_path.join(id.to_string());

        // Create the <KARL_PATH> if it does not already exist.
        fs::create_dir_all(&karl_path).unwrap();
        // Set the current working directory to the <KARL_PATH>.
        std::env::set_current_dir(&karl_path).unwrap();
        debug!("create karl_path {:?}", &karl_path);
        Self {
            id,
            karl_path,
            root,
            port,
            rt: Runtime::new().unwrap(),
        }
    }

    /// Process an incoming connection
    fn start(&mut self) -> Result<(), Error> {
        let listener = TcpListener::bind(format!("0.0.0.0:{}", self.port))?;
        self.port = listener.local_addr()?.port();
        info!("ID {} listening on port {}", self.id, self.port);
        register_service(&mut self.rt, self.port);
        for stream in listener.incoming() {
            let stream = stream?;
            debug!("incoming stream {:?}", stream.local_addr());
            let now = Instant::now();
            self.handle_client(stream).unwrap();
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
        unpack_request(&req, &self.root);
        let (mut config, root_path) = parse_request(&self.root);
        let import_paths = resolve_import_paths(&self.karl_path, &req.imports)?;
        config.binary_path = Some(resolve_binary_path(
            &config, &root_path, &import_paths)?);
        config.mapped_dirs = get_mapped_dirs(import_paths);
        info!("=> preprocessing: {} s", now.elapsed().as_secs_f32());

        // Replay the packaged computation.
        // Create the _compute_ working directory but stay in the karl path.
        let now = Instant::now();
        let mut options = Run::new(root_path);
        let result = replay_with_config(&mut options, config)
            .expect("expected result");
        info!("=> execution: {} s", now.elapsed().as_secs_f32());

        // Return the requested results.
        let now = Instant::now();
        let mut res = ComputeResult::new();
        if req.stdout {
            res.stdout = result.stdout;
        }
        if req.stderr {
            res.stderr = result.stderr;
        }
        for path in req.files {
            let f = self.root.join(&path);
            match fs::File::open(&f) {
                Ok(mut file) => {
                    res.files.insert(path, read_all(&mut file)?);
                },
                Err(e) => warn!("error opening output file {:?}: {:?}", f, e),
            }
        }
        info!("=> build result: {} s", now.elapsed().as_secs_f32());

        // Reset the root for the next computation.
        let now = Instant::now();
        std::env::set_current_dir(&self.karl_path).unwrap();
        std::fs::remove_dir_all(&self.root).unwrap();
        info!("reset directory at {:?} => {} s", self.root, now.elapsed().as_secs_f32());
        Ok(res)
    }

    /// Handle an incoming TCP stream.
    fn handle_client(&mut self, mut stream: TcpStream) -> Result<(), Error> {
        // Read the computation request from the TCP stream.
        let now = Instant::now();
        debug!("reading packet");
        let buf = read_packets(&mut stream, 1)?.remove(0);
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
        write_packet(&mut stream, &res_bytes)?;
        debug!("=> {} s", now.elapsed().as_secs_f32());
        Ok(())
    }
}

/// Register the service named "MyRustService" of type "_karl._tcp" via dns-sd.
///
/// TODO: might be platform dependent.
fn register_service(rt: &mut Runtime, port: u16) {
    rt.spawn(async move {
        let mut service = DNSServiceBuilder::new("_karl._tcp")
            .with_port(port)
            .with_name("MyRustService")
            .build()
            .unwrap();
        let _result = service.register(|reply| match reply {
            Ok(reply) => info!("successful register: {:?}", reply),
            Err(e) => info!("error registering: {:?}", e),
        });
        loop {
            if service.has_data() {
                service.process_result();
            }
        }
    });
}

fn main() {
    env_logger::builder().format_timestamp(None).init();
    let mut listener = Listener::new(0);
    listener.start().unwrap();
}
