use std::collections::HashSet;
use std::path::PathBuf;
use serde::{Serialize, Deserialize};

/// Requests.
#[derive(Debug, Serialize, Deserialize)]
pub enum KarlRequest {
    Ping(PingRequest),
    Compute(ComputeRequest),
}

/// Ping request.
#[derive(Debug, Serialize, Deserialize)]
pub struct PingRequest {}

/// Compute request.
#[derive(Debug, Serialize, Deserialize)]
pub struct ComputeRequest {
    /// Formatted zipped directory.
    ///
    /// config
    /// package/
    /// -- binary.wasm
    /// -- files
    pub zip: Vec<u8>,
    /// Whether to include stdout in the results.
    pub stdout: bool,
    /// Whether to include stderr in the results.
    pub stderr: bool,
    /// Files to include in the results, if they exist.
    pub files: HashSet<PathBuf>,
}

impl PingRequest {
    /// Create a new ping request.
    pub fn new() -> Self {
        PingRequest {}
    }
}

impl ComputeRequest {
    /// Create a new compute request based on a a serialized version of a
    /// formatted zipped directory.
    ///
    /// config
    /// package/
    /// -- binary.wasm
    /// -- files
    pub fn new(zip: Vec<u8>) -> Self {
        unimplemented!()
    }

    /// Include stdout in the results.
    pub fn stdout(&mut self) -> Self {
        unimplemented!()
    }

    /// Include stderr in the results.
    pub fn stderr(&mut self) -> Self {
        unimplemented!()
    }

    /// Include a specific output file in the results, if it exists.
    pub fn file(&mut self, filename: PathBuf) -> Self {
        unimplemented!()
    }
}
