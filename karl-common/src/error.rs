//! Custom karl-related errors.
use std::io;

#[derive(Debug)]
pub enum Error {
    /// I/O error.
    IoError(io::Error),
    /// Something is not found,
    NotFound,
    NotFoundInfo(String),
    BadRequest,
    BadRequestInfo(String),
    AlreadyExists,
    Unauthenticated,
    /// Failure to use persistent storage for request.
    StorageError(String),
    /// Error with module cache.
    CacheError(String),
    /// Error installing a module from a global module ID.
    ModuleInstallError(String),
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
