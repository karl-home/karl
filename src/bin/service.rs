// use wasmer::wasmer_runtime_core::pkg::Pkg
use std::fs;
use std::io;
use std::io::{Read, Write, BufRead};
use std::net::{TcpListener, TcpStream};
use std::time::Instant;

use wasmer::executor::{run, Run};
use tempdir::TempDir;
use zip;

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

fn handle_client(mut stream: TcpStream) -> io::Result<()> {
    // Read the computation request from the TCP stream.
    // WARNING: blocking
    let now = Instant::now();
    let buf = read_all(&mut stream)?;
    println!("read {} bytes: {} s", buf.len(), now.elapsed().as_secs_f32());

    // Decompress the request package into a temporary directory.
    let now = Instant::now();
    let reader = io::Cursor::new(buf);
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

    // TODO: Parse the return options from the TCP stream.
    // TODO: Serialize and return the requested results.
    Ok(())
}

fn main() -> io::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:0")?;
    println!("listening on port {}", listener.local_addr()?.port());

    // accept connections and process them serially
    for stream in listener.incoming() {
        handle_client(stream?)?;
    }
    Ok(())
}
