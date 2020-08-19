use std::io::{self, prelude::*};
use std::net::TcpStream;

fn main() -> io::Result<()> {
    let host_ip = "127.0.0.1";  // TODO: discover host IP through dns-sd
    let port = "61000";
    let mut stream = TcpStream::connect(format!("{}:{}", host_ip, port))?;

    stream.write(&[1])?;
    stream.read(&mut [0; 128])?;
    Ok(())
}
