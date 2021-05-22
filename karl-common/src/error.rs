//! Custom karl-related errors.
use std::io;
use tonic::{Status, Code};

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
    /// Unexpected packet type.
    InvalidPacketType(i32),
    /// Received a ping result for a compute request or vice versa.
    InvalidResponseType,
    /// Something is not found,
    NotFound,
    BadRequest,
    BadRequestInfo(String),
    AlreadyExists,
    InvalidArgument,
    Unauthenticated,
    /// Failure to install an imported package.
    InstallImportError(String),
    /// Failure to use persistent storage for request.
    StorageError(String),
    /// Error processing proxy request.
    ProxyError(String),
    /// Error with hook cache.
    CacheError(String),
    /// Unable to verify that a NotifyStart, NotifyEnd, or HostHeartbeat
    /// message came from a real host.
    InvalidHostMessage(String),
    /// Error installing a hook from a global hook ID.
    HookInstallError(String),
    /// Error authenticating a token.
    AuthError(String),
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

impl Error {
    pub fn to_tonic(self) -> Status {
        let (code, string) = match self {
            Error::NotFound => (Code::NotFound, None),
            Error::BadRequest => (Code::InvalidArgument, None),
            Error::BadRequestInfo(x) => (Code::InvalidArgument, Some(x)),
            Error::AlreadyExists => (Code::AlreadyExists, None),
            Error::Unauthenticated => (Code::Unauthenticated, None),
            _ => (Code::Unknown, None),
        };
        Status::new(code, string.unwrap_or("".to_string()))
    }
}
