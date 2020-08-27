use std::io;
use std::io::{BufRead, Read, Write};

#[derive(Debug)]
pub enum Error {
    IoError(io::Error),
    SerializationError(String),
    MissingPacketHeaderError,
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Error::IoError(error)
    }
}

/// Read bytes from the stream into a buffer.
///
/// If `max_nbytes` is provided, reads that number of bytes or until the
/// internal buffer is empty. Otherwise reads to EOF.
/// TODO: Maximum value of max_nbytes.
pub fn read_bytes(inner: &mut dyn Read, max_nbytes: Option<usize>) -> io::Result<Vec<u8>> {
    let mut buffer = Vec::new();
    let mut reader = io::BufReader::new(inner);
    // WARNING: blocking
    loop {
        let mut inner = reader.fill_buf()?.to_vec();
        if inner.len() == 0 {
            break;
        }
        if let Some(max_nbytes) = max_nbytes {
            let nbytes_remaining = max_nbytes - buffer.len();
            if inner.len() >= nbytes_remaining {
                inner.truncate(nbytes_remaining);
                reader.consume(nbytes_remaining);
                buffer.append(&mut inner);
                break;
            }
        }
        reader.consume(inner.len());
        buffer.append(&mut inner);
    }
    Ok(buffer)
}

/// Read bytes from the stream into a buffer. The first byte in the stream
/// is the packet length. TODO: maximum packet length.
pub fn read_packet(inner: &mut dyn Read) -> Result<Vec<u8>, Error> {
    info!("reading packet...");
    let nbytes = {
        let nbytes = read_bytes(inner, Some(1))?;
        if nbytes.is_empty() {
            return Err(Error::MissingPacketHeaderError);
        }
        *nbytes.get(0).unwrap() as usize
    };
    debug!("packet header: {}", nbytes);
    let res = read_bytes(inner, Some(nbytes))?;
    info!("read success!");
    Ok(res)
}

/// Write bytes into a stream. Include the packet length as the first byte.
pub fn write_packet(inner: &mut dyn Write, buffer: &Vec<u8>) -> io::Result<()> {
    info!("writing packet... ({} bytes)", buffer.len());
    let nbytes = buffer.len() as u8;
    inner.write_all(&[nbytes])?;
    inner.write_all(buffer)?;
    info!("write success!");
    Ok(())
}
