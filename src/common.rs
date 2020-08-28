use std::io;
use std::io::{BufRead, Read, Write};
use std::convert::TryInto;
use zip;

#[derive(Debug)]
pub enum Error {
    IoError(io::Error),
    ZipError(zip::result::ZipError),
    SerializationError(String),
    IncorrectPacketLength {
        actual: usize,
        expected: usize,
    },
    MissingHeader,
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Error::IoError(error)
    }
}

impl From<zip::result::ZipError> for Error {
    fn from(error: zip::result::ZipError) -> Self {
        Error::ZipError(error)
    }
}

/// Packet header length as u32.
const HEADER_LEN: usize = 4;

/// Read bytes from the stream into a buffer.
///
/// If no packet header is expected, reads until EOF. Otherwise, interprets
/// the first byte as the packet length and returns only the packet contents.
/// Returns Error::IncorrectPacketLength if the number of bytes read does not
/// match the epacket length.
///
/// WARNING: blocking
pub fn read_packet(
    inner: &mut dyn Read,
    header: bool,
) -> Result<Vec<u8>, Error> {
    let mut buffer = Vec::new();
    let mut reader = io::BufReader::new(inner);
    let mut nbytes: Option<usize> = None;
    if header {
        info!("reading packet...");
    }
    loop {
        let mut inner = reader.fill_buf()?.to_vec();
        trace!("read {} bytes", inner.len());
        if inner.len() == 0 {
            if header {
                debug!("EOF");
            }
            break;
        }

        // Parse the header if it hasn't been parsed already.
        if header && nbytes.is_none() {
            if inner.len() < HEADER_LEN {
                return Err(Error::MissingHeader);
            }
            let header: &[u8; HEADER_LEN] =
                &inner[..HEADER_LEN].try_into().unwrap();
            nbytes = Some(u32::from_ne_bytes(*header) as usize);
            inner = inner.split_off(HEADER_LEN);
            reader.consume(HEADER_LEN);
            debug!("packet header: {:?} {} bytes", header, nbytes.unwrap())
        }

        // Append the remaining bytes to the original buffer.
        reader.consume(inner.len());
        buffer.append(&mut inner);

        // Check if the packet is complete.
        if header && buffer.len() >= nbytes.unwrap() {
            break;
        }
    }

    // Handle incorrect packet lengths
    if header {
        if nbytes.is_none() {
            return Err(Error::MissingHeader);
        }
        if buffer.len() != nbytes.unwrap() {
            return Err(Error::IncorrectPacketLength {
                actual: buffer.len(),
                expected: nbytes.unwrap(),
            });
        }
        info!("read success! {} bytes", buffer.len());
    }
    Ok(buffer)
}

/// Write bytes into a stream. Include the packet length as the first byte.
pub fn write_packet(inner: &mut dyn Write, buffer: &Vec<u8>) -> io::Result<()> {
    info!("writing packet... ({} bytes)", buffer.len());
    // TODO: what if length doesn't fit in u32?
    let nbytes = (buffer.len() as u32).to_ne_bytes();
    info!("writing {:?}", nbytes);
    inner.write_all(&nbytes)?;
    inner.write_all(buffer)?;
    info!("write success!");
    Ok(())
}
