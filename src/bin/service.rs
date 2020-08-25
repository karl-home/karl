// use wasmer::wasmer_runtime_core::pkg::Pkg
use std::fs;
use std::io;
use std::io::{Read, Write, BufRead};
use std::net::{TcpListener, TcpStream};
use std::time::Instant;

use bincode;
use wasmer::executor::{run, Run};
use tempdir::TempDir;
use zip;

use karl::*;

#[derive(Debug)]
enum Error {
    IoError(io::Error),
    SerializationError(String),
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Error::IoError(error)
    }
}

fn read_all(inner: &mut dyn Read) -> io::Result<Vec<u8>> {
    let mut buffer = Vec::new();
    let mut reader = io::BufReader::new(inner);
    // WARNING: blocking
    loop {
        let mut inner = reader.fill_buf()?.to_vec();
        if inner.len() == 0 {
            break;
        }
        reader.consume(inner.len());
        buffer.append(&mut inner);
    }
    Ok(buffer)
}

fn handle_ping(req: PingRequest) -> PingResult {
    unimplemented!()
}

fn handle_compute(req: ComputeRequest) -> io::Result<ComputeResult> {
    // Decompress the request package into a temporary directory.
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
            let buf = read_all(&mut file)?;
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
    let buf = read_all(&mut stream)?;
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
    let res_bytes = bincode::serialize(&res)
        .map_err(|e| Error::SerializationError(format!("{:?}", e)))?;
    stream.write(&res_bytes[..])?;
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
