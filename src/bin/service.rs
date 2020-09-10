#[macro_use]
extern crate log;

use std::fs;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Instant;

use bincode;
use tempfile;
use tokio::runtime::Runtime;
use astro_dnssd::register::DNSServiceBuilder;
use wasmer::executor::{run, Run};

use karl::*;

struct Listener {
    /// Node/service ID
    id: u32,
    /// Root karl directory
    karl_path: PathBuf,
    port: u16,
    rt: Runtime,
}

impl Listener {
    fn new(port: u16) -> Self {
        use rand::Rng;
        Self {
            id: rand::thread_rng().gen(),
            karl_path: Path::new("~/.karl").to_path_buf(),
            port,
            rt: Runtime::new().unwrap(),
        }
    }

    /// Process an incoming connection
    fn start(&mut self) -> Result<(), Error> {
        let listener = TcpListener::bind(format!("0.0.0.0:{}", self.port))?;
        self.port = listener.local_addr()?.port();
        info!("listening on port {}", self.port);
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
        // Write the tar.gz bytes into a temporary file.
        info!("handling compute: (len {}) stdout={} stderr={} {:?}",
            req.package.len(), req.stdout, req.stderr, req.files);
        let now = Instant::now();
        let mut f = tempfile::NamedTempFile::new()?;
        f.write_all(&req.package[..])?;
        f.flush()?;
        info!("=> write package.tar.gz: {} s", now.elapsed().as_secs_f32());

        // Replay the packaged computation.
        let now = Instant::now();
        let mut options = Run::new(f.path().to_path_buf());
        options.replay = true;
        let result = run(&mut options).expect("expected result");
        info!("=> execution: {} s", now.elapsed().as_secs_f32());

        // Remove the file and return the requested results.
        // TODO: KARL_PATH
        f.close()?;
        let now = Instant::now();
        let mut res = ComputeResult::new();
        if req.stdout {
            res.stdout = result.stdout;
        }
        if req.stderr {
            res.stderr = result.stderr;
        }
        for path in req.files {
            // TODO: use KARL_PATH environment variable
            let f = result.root.path().join(&path);
            match fs::File::open(&f) {
                Ok(mut file) => {
                    res.files.insert(path, read_packet(&mut file, false)?);
                },
                Err(e) => warn!("error opening output file {:?}: {:?}", f, e),
            }
        }
        info!("=> build result: {} s", now.elapsed().as_secs_f32());
        Ok(res)
    }

    /// Handle an incoming TCP stream.
    fn handle_client(&mut self, mut stream: TcpStream) -> Result<(), Error> {
        // Read the computation request from the TCP stream.
        let now = Instant::now();
        debug!("reading packet");
        let buf = read_packet(&mut stream, true)?;
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

fn main() -> Result<(), Error> {
    env_logger::builder().format_timestamp(None).init();
    let mut listener = Listener::new(0);
    listener.start()?;
    Ok(())
}
