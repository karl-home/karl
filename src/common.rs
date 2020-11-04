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
    /// Failure to install an imported package.
    InstallImportError(String),
    /// Unknown.
    UnknownError(String),
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Error::IoError(error)
    }
}

impl From<String> for Error {
    fn from(error: String) -> Self {
        Error::UnknownError(error)
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
                let buffer: Vec<_> = current_buffer.drain(..nbytes).collect();
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
    if result.is_empty() && current_buffer.is_empty() && current_nbytes.is_none() {
        return Err(Error::NoReply);
    }
    // Handle incorrect packet lengths
    assert!(result.len() <= npackets);
    if result.len() < npackets {
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

#[cfg(test)]
mod test {
    use super::*;
    use std::fs;
    use std::path::Path;

    #[test]
    fn write_packet_short_header() {
        let mut buf = vec![];
        let input = b"helloworld".to_vec();
        match write_packet(&mut buf, &input) {
            Ok(()) => {},
            Err(e) => assert!(false, format!("failed to write packet: {:?}", e)),
        }
        assert!(HEADER_LEN > 0);
        assert_eq!(input.len() + HEADER_LEN, buf.len(), "wrong number of bytes");
        assert_eq!(buf[HEADER_LEN..].to_vec(), input, "input was written incorrectly");
        let header: &[u8; HEADER_LEN] = &buf[..HEADER_LEN].try_into().unwrap();
        let nbytes = u32::from_ne_bytes(*header);
        assert_eq!(nbytes as usize, input.len(), "incorrect header value");
    }

    #[test]
    fn write_packet_long_header() {
        let mut buf = vec![];
        let audio_file = Path::new("data/stt_node/weather.wav");
        assert!(audio_file.exists(), "run scripts/setup_stt_node.sh");
        let input = fs::read(audio_file).unwrap();
        assert!(input.len() > 65535, "input length still fits in u16");
        match write_packet(&mut buf, &input) {
            Ok(()) => {},
            Err(e) => assert!(false, format!("failed to write packet: {:?}", e)),
        }
        assert!(HEADER_LEN > 0);
        assert_eq!(input.len() + HEADER_LEN, buf.len(), "wrong number of bytes");
        assert_eq!(buf[HEADER_LEN..].to_vec(), input, "input was written incorrectly");
        let header: &[u8; HEADER_LEN] = &buf[..HEADER_LEN].try_into().unwrap();
        // if the packet length doesn't fit in u32... too bad
        let nbytes = u32::from_ne_bytes(*header);
        assert_eq!(nbytes as usize, input.len(), "incorrect header value");
    }

    #[test]
    fn read_one_packet() {
        let mut buf = vec![];
        let mut file = fs::File::open("data/stt_node/weather.wav").unwrap();
        let input = read_all(&mut file).unwrap();
        assert_eq!(input.len(), 130842);
        // assume write packet works correctly
        write_packet(&mut buf, &input).unwrap();
        let mut cursor = io::Cursor::new(buf);
        let packets = match read_packets(&mut cursor, 1) {
            Ok(packets) => packets,
            Err(e) => {
                assert!(false, format!("{:?}", e));
                unreachable!();
            },
        };
        assert_eq!(packets.len(), 1, "expected 1 packet");
        assert_eq!(packets[0].len(), 130842);
        assert_eq!(packets[0], input);
    }

    #[test]
    fn read_two_packets() {
        let mut buf = vec![];
        let mut file1 = fs::File::open("data/stt_node/weather.wav").unwrap();
        let input1 = read_all(&mut file1).unwrap();
        assert_eq!(input1.len(), 130842);
        let mut file2 = fs::File::open("data/stt/audio/2830-3980-0043.wav").unwrap();
        let input2 = read_all(&mut file2).unwrap();
        assert_eq!(input2.len(), 63244);
        // assume write packet works correctly
        write_packet(&mut buf, &input1).unwrap();
        write_packet(&mut buf, &input2).unwrap();
        assert_eq!(buf.len(), 130842 + 63244 + HEADER_LEN * 2);
        let mut cursor = io::Cursor::new(buf);
        let packets = match read_packets(&mut cursor, 2) {
            Ok(packets) => packets,
            Err(e) => {
                assert!(false, format!("{:?}", e));
                unreachable!();
            },
        };
        assert_eq!(packets.len(), 2, "expected 2 packets");
        assert_eq!(packets[0].len(), 130842);
        assert_eq!(packets[0], input1);
        assert_eq!(packets[1].len(), 63244);
        assert_eq!(packets[1], input2);
    }

    #[test]
    fn read_empty_buffer() {
        let buf = vec![];
        let mut cursor = io::Cursor::new(buf);
        match read_packets(&mut cursor, 1) {
            Ok(_) => assert!(false, "expected incorrect number of packets"),
            Err(e) => match e {
                Error::NoReply => {},
                e => assert!(false, format!("unexpected error: {:?}", e)),
            },
        }
    }

    #[test]
    fn read_too_many_packets() {
        let mut buf = vec![];
        let mut file = fs::File::open("data/stt/audio/2830-3980-0043.wav").unwrap();
        let input = read_all(&mut file).unwrap();
        assert_eq!(input.len(), 63244);
        // assume write packet works correctly
        write_packet(&mut buf, &input).unwrap();
        let mut cursor = io::Cursor::new(buf);
        match read_packets(&mut cursor, 2) {
            Ok(_) => assert!(false, "expected incorrect number of packets"),
            Err(e) => match e {
                Error::IncorrectNumPackets { actual, expected } => {
                    assert_eq!(expected, 2);
                    assert_eq!(actual, 1);
                },
                e => assert!(false, format!("unexpected error: {:?}", e)),
            },
        }
    }

    #[test]
    fn read_incorrect_packet_length() {
        let mut buf = vec![];
        let mut file = fs::File::open("data/stt_node/weather.wav").unwrap();
        let input = read_all(&mut file).unwrap();
        assert_eq!(input.len(), 130842);
        write_packet(&mut buf, &input).unwrap();
        // pretend some bytes got lost at the end
        assert_eq!(buf.len(), 130842 + HEADER_LEN);
        let _ = buf.split_off(130840 + HEADER_LEN);
        assert!(buf.len() < 130842 + HEADER_LEN);
        let mut cursor = io::Cursor::new(buf);
        match read_packets(&mut cursor, 1) {
            Ok(_) => assert!(false, "expected incorrect packet length"),
            Err(e) => match e {
                Error::IncorrectPacketLength { actual, expected } => {
                    assert_eq!(expected, 130842);
                    assert_eq!(actual, 130840);
                },
                e => assert!(false, format!("unexpected error: {:?}", e)),
            },
        }
    }
}
