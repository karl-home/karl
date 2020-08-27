use std::io;
use std::io::{BufRead, Read, Write};
use zip;

#[derive(Debug)]
pub enum Error {
    IoError(io::Error),
    ZipError(zip::result::ZipError),
    SerializationError(String),
    IncorrectPacketLength,
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
    info!("reading packet... header={}", header);
    loop {
        let mut inner = reader.fill_buf()?.to_vec();
        trace!("read {} bytes", inner.len());
        if inner.len() == 0 {
            debug!("EOF");
            break;
        }
        if header && buffer.len() == 0 {
            debug!("packet header: {} bytes", inner[0])
        }
        reader.consume(inner.len());
        buffer.append(&mut inner);

        // Expecting a packet header, check if packet is complete
        if header {
            let nbytes = buffer.get(0).unwrap();
            if buffer.len() >= (nbytes + 1).into() {
                break;
            }
        }
    }

    // Handle incorrect packet lengths
    if header {
        if buffer.len() == 0 {
            return Err(Error::MissingHeader);
        }
        let nbytes = buffer.remove(0);
        if buffer.len() != nbytes.into() {
            return Err(Error::IncorrectPacketLength);
        }
    }
    info!("read success! {} bytes", buffer.len());
    Ok(buffer)
}

/// Write bytes into a stream. Include the packet length as the first byte.
pub fn write_packet(inner: &mut dyn Write, buffer: &Vec<u8>) -> io::Result<()> {
    info!("writing packet... ({} bytes)", buffer.len());
    let nbytes = buffer.len() as u8;
    info!("writing {:?}", &[nbytes]);
    inner.write_all(&[nbytes])?;
    inner.write_all(buffer)?;
    info!("write success!");
    Ok(())
}
