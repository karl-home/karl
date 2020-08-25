use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Serialize, Deserialize};

/// Results.
#[derive(Debug, Serialize, Deserialize)]
pub enum KarlResult {
    Ping(PingResult),
    Compute(ComputeResult),
}

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
    pub files: HashMap<PathBuf, Vec<u8>>,
}

impl ComputeResult {
    pub fn new() -> Self {
        unimplemented!()
    }
}
