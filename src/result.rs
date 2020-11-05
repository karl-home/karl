use std::collections::HashMap;
use serde::{Serialize, Deserialize};

/// Ping result.
#[derive(Debug, Serialize, Deserialize)]
pub struct PingResult {}

/// Compute result.
#[derive(Debug, Serialize, Deserialize)]
pub struct ComputeResult {
    /// Stdout.
    pub stdout: Vec<u8>,
    /// Stderr.
    pub stderr: Vec<u8>,
    /// Files.
    pub files: HashMap<String, Vec<u8>>,
}

impl PingResult {
    pub fn new() -> Self {
        PingResult {}
    }
}

impl ComputeResult {
    pub fn new() -> Self {
        ComputeResult {
            stdout: Vec::new(),
            stderr: Vec::new(),
            files: HashMap::new(),
        }
    }
}
