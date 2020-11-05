use std::io;
use std::fs;
use std::fmt;
use std::collections::HashSet;
use std::path::Path;

use serde::{Serialize, Deserialize};
use tempdir::TempDir;
use tar::{Builder, Header};
use flate2::{Compression, write::GzEncoder};

use crate::{Error, Import, PkgConfig};

/// Ping request.
#[repr(C)]
#[derive(Debug, Serialize, Deserialize)]
pub struct PingRequest {}

/// Ping result.
#[repr(C)]
#[derive(Debug, Serialize, Deserialize)]
pub struct PingResult {}

/// Host request.
#[repr(C)]
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct HostRequest {}

/// Host result.
#[repr(C)]
#[derive(Debug, Serialize, Deserialize)]
pub struct HostResult {
    pub ip: String,
    pub port: String,
}

/// Compute request builder.
#[repr(C)]
#[derive(Debug, Serialize, Deserialize)]
pub struct ComputeRequestBuilder {
    pub dirs: Vec<String>,
    pub files: Vec<String>,
    pub imports: Vec<Import>,
    pub config: PkgConfig,
}

/// Compute request.
#[repr(C)]
#[derive(Serialize, Deserialize)]
pub struct ComputeRequest {
    /// Formatted tar.gz directory.
    ///
    /// config
    /// package/
    /// -- binary.wasm
    /// -- files
    pub package: Vec<u8>,
    /// Whether to include stdout in the results.
    pub stdout: bool,
    /// Whether to include stderr in the results.
    pub stderr: bool,
    /// Files to include in the results, if they exist.
    pub files: HashSet<String>,
    /// Imported packages or libraries
    pub imports: Vec<Import>,
}

/// Compute result.
#[repr(C)]
#[derive(Debug, Serialize, Deserialize)]
pub struct ComputeResult {
    /// Stdout.
    pub stdout: Vec<u8>,
    /// Stderr.
    pub stderr: Vec<u8>,
    /// Files.
    pub files: Vec<(String, Vec<u8>)>,
}

impl PingRequest {
    /// Create a new ping request.
    pub fn new() -> Self {
        PingRequest {}
    }
}

impl PingResult {
    pub fn new() -> Self {
        PingResult {}
    }
}

fn cp(root: &Path, path: &Path) -> io::Result<()> {
    if path.is_dir() {
        fs::create_dir_all(root.join(path))?;
        for entry in fs::read_dir(path)? {
            cp(root, &entry?.path())?;
        }
    } else {
        fs::copy(path, root.join(path))?;
    }
    Ok(())
}

impl ComputeRequestBuilder {
    pub fn new(binary_path: &str) -> ComputeRequestBuilder {
        ComputeRequestBuilder {
            dirs: Vec::new(),
            files: Vec::new(),
            imports: Vec::new(),
            config: PkgConfig {
                binary_path: Some(Path::new(binary_path).to_path_buf()),
                mapped_dirs: Vec::new(),
                args: Vec::new(),
                envs: Vec::new(),
            },
        }
    }

    /// Arguments, not including the binary path
    pub fn args(mut self, args: Vec<&str>) -> ComputeRequestBuilder {
        self.config.args = args
            .into_iter()
            .map(|arg| arg.to_string())
            .collect();
        self
    }

    /// Environment variables in the format <ENV_VARIABLE>=<VALUE>
    pub fn envs(mut self, envs: Vec<&str>) -> ComputeRequestBuilder {
        self.config.envs = envs
            .into_iter()
            .map(|env| env.to_string())
            .collect();
        self
    }

    /// Import external library or package.
    pub fn import(mut self, import: Import) -> ComputeRequestBuilder {
        self.imports.push(import);
        self
    }

    /// Add a file to the input root from the home filesystem, overwriting
    /// files with the same name.
    pub fn add_file(mut self, path: &str) -> ComputeRequestBuilder {
        self.files.push(path.to_string());
        self
    }

    /// Add a directory to the input root from the home filesystem, overwriting
    /// files with the same name.
    pub fn add_dir(mut self, path: &str) -> ComputeRequestBuilder {
        self.dirs.push(path.to_string());
        self
    }

    /// Finalize the compute request.
    pub fn finalize(self) -> Result<ComputeRequest, Error> {
        // Create input root.
        let root = TempDir::new("karl").unwrap();
        for path in &self.dirs {
            cp(root.path(), Path::new(&path))?;
        }
        for path in &self.files {
            let new_path = root.path().join(path);
            let parent = new_path.parent().unwrap();
            fs::create_dir_all(parent)?;
            fs::copy(path, new_path)?;
        }

        // Tar it up.
        let mut buffer = Vec::new();
        let enc = GzEncoder::new(&mut buffer, Compression::default());
        let mut tar = Builder::new(enc);
        tar.append_dir_all("root", root.path())?;

        // Tar the config.
        let config = bincode::serialize(&self.config).unwrap();
        let mut header = Header::new_gnu();
        header.set_size(config.len() as _);
        header.set_cksum();
        tar.append_data(&mut header, "config", &config[..])?;
        tar.into_inner()?;

        // Generate the default compute request.
        let mut request = ComputeRequest::new(buffer);
        request.imports = self.imports;
        Ok(request)
    }
}

impl fmt::Debug for ComputeRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ComputeRequest")
            .field("package (nbytes)", &self.package.len())
            .field("stdout", &self.stdout)
            .field("stderr", &self.stderr)
            .field("files", &self.files)
            .finish()
    }
}

impl ComputeRequest {
    /// Create a new compute request based on a a serialized version of a
    /// formatted tar.gz directory.
    ///
    /// config
    /// package/
    /// -- binary.wasm
    /// -- files
    pub fn new(tar_gz_bytes: Vec<u8>) -> Self {
        ComputeRequest {
            package: tar_gz_bytes,
            stdout: false,
            stderr: false,
            files: HashSet::new(),
            imports: Vec::new(),
        }
    }

    /// Include stdout in the results.
    pub fn stdout(mut self) -> Self {
        self.stdout = true;
        self
    }

    /// Include stderr in the results.
    pub fn stderr(mut self) -> Self {
        self.stderr = true;
        self
    }

    /// Include a specific output file in the results, if it exists.
    pub fn file(mut self, filename: &str) -> Self {
        self.files.insert(filename.to_string());
        self
    }
}

impl ComputeResult {
    pub fn new() -> Self {
        ComputeResult {
            stdout: Vec::new(),
            stderr: Vec::new(),
            files: Vec::new(),
        }
    }
}
