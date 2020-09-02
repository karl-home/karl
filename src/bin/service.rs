#[macro_use]
extern crate log;

use std::fs;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::time::Instant;

use bincode;
use async_dnssd::{RegisterData, Registration, RegisterResult};
use tokio_core::reactor::Core;
use wasmer::executor::{run, Run};

use karl::*;

/// Handle a ping request.
fn handle_ping(req: PingRequest) -> PingResult {
    info!("handling ping: {:?}", req);
    PingResult::new()
}

/// Handle a compute request.
fn handle_compute(req: ComputeRequest) -> Result<ComputeResult, Error> {
    // Write the tar.gz bytes into a temporary file.
    info!("handling compute: {} {} {} {:?}",
        req.package.len(), req.stdout, req.stderr, req.files);
    let now = Instant::now();
    let filename = "package.tar.gz";
    let mut f = fs::File::create(filename)?;
    f.write_all(&req.package[..])?;
    f.flush()?;
    debug!("write {} bytes: {} s", req.package.len(), now.elapsed().as_secs_f32());

    // Replay the packaged computation.
    let now = Instant::now();
    let mut options = Run::new(std::path::Path::new(filename).to_path_buf());
    options.replay = true;
    let result = run(&mut options).expect("expected result");
    debug!("execution: {} s", now.elapsed().as_secs_f32());

    // Return the requested results.
    let mut res = ComputeResult::new();
    if req.stdout {
        res.stdout = result.stdout;
    }
    if req.stderr {
        res.stderr = result.stderr;
    }
    for path in req.files {
        let f = result.root.path().join(&path);
        match fs::File::open(&f) {
            Ok(mut file) => {
                res.files.insert(path, read_packet(&mut file, false)?);
            },
            Err(e) => warn!("error opening output file {:?}: {:?}", f, e),
        }
    }
    Ok(res)
}

/// Handle an incoming TCP stream.
fn handle_client(mut stream: TcpStream) -> Result<(), Error> {
    // Read the computation request from the TCP stream.
    let now = Instant::now();
    let buf = read_packet(&mut stream, true)?;
    info!("read {}-byte packet: {} s", buf.len(), now.elapsed().as_secs_f32());

    // Deserialize the request.
    let req_bytes = bincode::deserialize(&buf[..])
        .map_err(|e| Error::SerializationError(format!("{:?}", e)))?;

    // Deploy the request to correct handler.
    let res = match req_bytes {
        KarlRequest::Ping(req) => KarlResult::Ping(handle_ping(req)),
        KarlRequest::Compute(req) => KarlResult::Compute(handle_compute(req)?),
    };

    // Return the result to sender.
    info!("returning {:?}", res);
    let res_bytes = bincode::serialize(&res)
        .map_err(|e| Error::SerializationError(format!("{:?}", e)))?;
    write_packet(&mut stream, &res_bytes)?;
    Ok(())
}

/// Register the service named "karl" of type "_tcp._tcp" via dns-sd.
///
/// TODO: might be platform dependent.
fn register_service(core: &mut Core, port: u16) -> (Registration, RegisterResult) {
    use async_dnssd::{Interface, InterfaceIndex};
    let result = core.run(async_dnssd::register_extended(
        "_tcp._tcp",
        port,
        RegisterData {
            interface: Interface::Index(InterfaceIndex::from_raw(8).unwrap()),
            name: Some("karl"),
            ..Default::default()
        },
        &core.handle(),
    ).unwrap()).unwrap();
    info!("registered service");
    result
}

fn main() -> Result<(), Error> {
    env_logger::builder().format_timestamp(None).init();
    let listener = TcpListener::bind("0.0.0.0:0")?;
    let port = listener.local_addr()?.port();
    info!("listening on port {}", port);
    let mut core = Core::new().unwrap();
    let (registration, _) = register_service(&mut core, port);

    // accept connections and process them serially
    for stream in listener.incoming() {
        let stream = stream?;
        debug!("incoming stream {:?}", stream.local_addr());
        handle_client(stream)?;
    }
    drop(registration);
    Ok(())
}
