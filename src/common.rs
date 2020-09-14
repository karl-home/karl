use std::io;
use std::io::{BufRead, Read, Write};
use std::convert::TryInto;

#[derive(Debug)]
pub enum Error {
    /// I/O error.
    IoError(io::Error),
    /// Error serializing or deserializing a request or result.
    SerializationError(String),
    /// The number of bytes received before EOF does not correspond to
    /// the number of bytes indicated by the 4-byte header.
    IncorrectPacketLength {
        actual: usize,
        expected: usize,
    },
    /// The number of packets received does not correspond to the number
    /// of packets actually received.
    IncorrectNumPackets {
        actual: usize,
        expected: usize,
    },
    /// Expected to read a packet but received the connection closed
    /// and no bytes were received.
    NoReply,
    /// The packet does not have enough bytes to constitute a header.
    /// The header should include 4 bytes.
    MissingHeader,
    /// No available hosts.
    NoAvailableHosts,
    /// Invalid input root. Either the input root is uninitialized, or
    /// you initialized the root as an existing directory rather than a
    /// custom-built one.
    InvalidInputRoot,
    /// Reinitialized the input root. Should only initialize it once.
    DoubleInputInitialization,
    /// Received a ping result for a compute request or vice versa.
    InvalidResponseType,
    /// Package does not contain a valid binary in its root or imports.
    BinaryNotFound(String),
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Error::IoError(error)
    }
}

/// Packet header length as u32.
const HEADER_LEN: usize = 4;

/// Read bytes from the stream into multiple buffers.
///
/// Interprets the first HEADER_LEN bytes as the packet length and returns
/// only the packet contents. Errors:
/// - IncorrectPacketLength - header does not match packet length, or incorrect
///   number of packets.
/// - NoReply - connection closed before all packets were read.
///
/// WARNING: blocking
pub fn read_packets(
    inner: &mut dyn Read,
    npackets: usize,
) -> Result<Vec<Vec<u8>>, Error> {
    trace!("reading {} packets...", npackets);
    let mut result = Vec::new();
    let mut reader = io::BufReader::new(inner);

    let mut index = 0;
    let mut current_buffer = Vec::new();
    let mut current_nbytes: Option<usize> = None;
    loop {
        let mut inner = reader.fill_buf()?.to_vec();
        trace!("read {} bytes", inner.len());
        if inner.len() == 0 {
            trace!("EOF");
            break;
        }
        reader.consume(inner.len());
        current_buffer.append(&mut inner);

        loop {
            // Parse the header if necessary
            if current_nbytes.is_none() {
                if current_buffer.len() >= HEADER_LEN {
                    let header: &[u8; HEADER_LEN] =
                        &current_buffer[..HEADER_LEN].try_into().unwrap();
                    current_nbytes = Some(u32::from_ne_bytes(*header) as usize);
                    current_buffer = current_buffer.split_off(HEADER_LEN);
                    trace!("packet header: {:?} {} bytes", header, current_nbytes.unwrap())
                } else {
                    // Need more bytes
                    break;
                }
            }

            // Split off a packet if there are enough bytes
            let nbytes = current_nbytes.unwrap();
            if current_buffer.len() >= nbytes {
                let buffer: Vec<_> = current_buffer.drain(..).collect();
                result.push(buffer);
                current_nbytes = None;
                index += 1;
                trace!("finished packet {} bytes", nbytes);
                // Short circuit if read the last packet
                if index == npackets {
                    break;
                }
            } else {
                // Need more bytes
                break;
            }
        }

        if index == npackets {
            break;
        }
    }

    // Handle no reply, wrong number of replies
    if result.is_empty() {
        return Err(Error::NoReply);
    }
    // Handle incorrect packet lengths
    assert!(result.len() <= npackets);
    if result.len() == 0 {
        Err(Error::NoReply)
    } else if result.len() < npackets {
        if let Some(nbytes) = current_nbytes {
            Err(Error::IncorrectPacketLength {
                actual: current_buffer.len(),
                expected: nbytes,
            })
        } else {
            Err(Error::IncorrectNumPackets {
                actual: result.len(),
                expected: npackets,
            })
        }
    } else {
        trace!("read success! {:?} bytes",
            result.iter().map(|a| a.len()).collect::<Vec<_>>());
        Ok(result)
    }
}

/// Read bytes from the stream into a buffer. Reads until EOF.
///
/// WARNING: blocking
pub fn read_all(
    inner: &mut dyn Read,
) -> Result<Vec<u8>, Error> {
    let mut buffer = Vec::new();
    let mut reader = io::BufReader::new(inner);
    loop {
        let mut inner = reader.fill_buf()?.to_vec();
        trace!("read {} bytes", inner.len());
        if inner.len() == 0 {
            break;
        }
        reader.consume(inner.len());
        buffer.append(&mut inner);
    }
    if buffer.is_empty() {
        return Err(Error::NoReply);
    }
    Ok(buffer)
}

/// Write bytes into a stream. Include the packet length as the first byte.
pub fn write_packet(inner: &mut dyn Write, buffer: &Vec<u8>) -> io::Result<()> {
    trace!("writing packet... ({} bytes)", buffer.len());
    // TODO: what if length doesn't fit in u32?
    let nbytes = (buffer.len() as u32).to_ne_bytes();
    trace!("writing {:?}", nbytes);
    inner.write_all(&nbytes)?;
    inner.write_all(buffer)?;
    trace!("write success!");
    Ok(())
}
