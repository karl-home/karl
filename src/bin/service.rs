#[macro_use]
extern crate log;

use std::fs;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::time::Instant;

use bincode;
use tokio::runtime::Runtime;
use astro_dnssd::register::DNSServiceBuilder;
use wasmer::executor::{run, Run};

use karl::*;

/// Handle a ping request.
fn handle_ping(_req: PingRequest) -> PingResult {
    PingResult::new()
}

/// Handle a compute request.
fn handle_compute(req: ComputeRequest) -> Result<ComputeResult, Error> {
    // Write the tar.gz bytes into a temporary file.
    info!("handling compute: (len {}) stdout={} stderr={} {:?}",
        req.package.len(), req.stdout, req.stderr, req.files);
    let now = Instant::now();
    let filename = "package.tar.gz";
    let mut f = fs::File::create(filename)?;
    f.write_all(&req.package[..])?;
    f.flush()?;
    info!("=> write package.tar.gz: {} s", now.elapsed().as_secs_f32());

    // Replay the packaged computation.
    let now = Instant::now();
    let mut options = Run::new(std::path::Path::new(filename).to_path_buf());
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
fn handle_client(mut stream: TcpStream) -> Result<(), Error> {
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
        KarlRequest::Ping(req) => KarlResult::Ping(handle_ping(req)),
        KarlRequest::Compute(req) => KarlResult::Compute(handle_compute(req)?),
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
            service.process_result();
        }
    });
}

fn main() -> Result<(), Error> {
    env_logger::builder().format_timestamp(None).init();
    let listener = TcpListener::bind("0.0.0.0:0")?;
    let port = listener.local_addr()?.port();
    info!("listening on port {}", port);
    let mut rt = Runtime::new().unwrap();
    register_service(&mut rt, port);

    // accept connections and process them serially
    for stream in listener.incoming() {
        let stream = stream?;
        debug!("incoming stream {:?}", stream.local_addr());
        handle_client(stream)?;
    }
    Ok(())
}
