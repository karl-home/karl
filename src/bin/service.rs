#[macro_use]
extern crate log;

use std::fs;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Instant;

use bincode;
use dirs;
use tokio::runtime::Runtime;
use astro_dnssd::register::DNSServiceBuilder;
use wasmer::executor::{run, Run};
use flate2::read::GzDecoder;
use tar::Archive;

use karl::*;

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
            self.handle_client(stream)?;
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
        unpack_request(&req, &self.root);

        // Replay the packaged computation.
        // Create the _compute_ working directory but stay in the karl path.
        let now = Instant::now();
        let mut options = Run::new(self.root.clone());
        options.replay = true;
        let result = run(&mut options).expect("expected result");
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
