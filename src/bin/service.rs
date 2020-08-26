// use wasmer::wasmer_runtime_core::pkg::Pkg
use std::fs;
use std::io;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::time::Instant;

use bincode;
use wasmer::executor::{run, Run};
use tempdir::TempDir;
use zip;

use karl::*;

fn handle_ping(req: PingRequest) -> PingResult {
    println!("RX: ping {:?}", req);
    PingResult::new()
}

fn handle_compute(req: ComputeRequest) -> io::Result<ComputeResult> {
    // Decompress the request package into a temporary directory.
    println!("RX: compute");
    let now = Instant::now();
    let reader = io::Cursor::new(req.zip);
    let mut zip = zip::ZipArchive::new(reader)?;
    let package = TempDir::new("package")?;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        let path = package.path().join(file.sanitized_name());
        if file.is_dir() {
            fs::create_dir(path)?;
        } else {
            let mut out = fs::File::create(path)?;
            let buf = read_bytes(&mut file, None)?;
            out.write_all(&buf)?;
        }
    }
    println!("decompressed {} files: {} s", zip.len(), now.elapsed().as_secs_f32());

    // Replay the packaged computation.
    let now = Instant::now();
    let mut options = Run::new(package.path().to_path_buf());
    options.replay = true;
    run(&mut options);
    println!("execution: {} s", now.elapsed().as_secs_f32());

    // TODO: Serialize and return the requested results.
    Ok(ComputeResult::new())
}

fn handle_client(mut stream: TcpStream) -> Result<(), Error> {
    // Read the computation request from the TCP stream.
    let now = Instant::now();
    let buf = read_packet(&mut stream)?;
    println!("read {} bytes: {} s", buf.len(), now.elapsed().as_secs_f32());

    // Deserialize the request.
    let req_bytes = bincode::deserialize(&buf[..])
        .map_err(|e| Error::SerializationError(format!("{:?}", e)))?;

    // Deploy the request to correct handler.
    let res = match req_bytes {
        KarlRequest::Ping(req) => KarlResult::Ping(handle_ping(req)),
        KarlRequest::Compute(req) => KarlResult::Compute(handle_compute(req)?),
    };

    // Return the result to sender.
    println!("TX: {:?}", res);
    let res_bytes = bincode::serialize(&res)
        .map_err(|e| Error::SerializationError(format!("{:?}", e)))?;
    write_packet(&mut stream, &res_bytes)?;
    Ok(())
}

fn main() -> Result<(), Error> {
    let listener = TcpListener::bind("0.0.0.0:0")?;
    println!("listening on port {}", listener.local_addr()?.port());

    // accept connections and process them serially
    for stream in listener.incoming() {
        handle_client(stream?)?;
    }
    Ok(())
}
