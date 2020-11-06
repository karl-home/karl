use std::fs;
use std::fmt;
use std::collections::HashSet;
use std::path::Path;

use serde::{Serialize, Deserialize};
use tar::Builder;
use flate2::{Compression, write::GzEncoder};

use super::{Error, Import, PkgConfig};

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
    /// Input root directory formatted as tar.gz.
    ///
    /// Should be unpackaged into a directory called `root`.
    pub package: Vec<u8>,
    /// Package config.
    pub config: PkgConfig,
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
    ///
    /// TODO: needs to be a relative path. should be forward searching only.
    pub fn add_dir(mut self, path: &str) -> ComputeRequestBuilder {
        self.dirs.push(path.to_string());
        self
    }

    /// Finalize the compute request.
    pub fn finalize(self) -> Result<ComputeRequest, Error> {
        // Tar up the input root.
        let mut buffer = Vec::new();
        let enc = GzEncoder::new(&mut buffer, Compression::default());
        let mut tar = Builder::new(enc);
        for path in &self.dirs {
            tar.append_dir_all(path, path)?;
        }
        for path in &self.files {
            let path = Path::new(path);
            let parent = path.parent().unwrap();
            if parent.exists() {
                tar.append_dir(parent, parent)?;
            }
            tar.append_file(path, &mut fs::File::open(path)?)?;
        }

        // Generate the default compute request.
        drop(tar);
        Ok(ComputeRequest::new(
            buffer,
            self.config,
            self.imports,
        ))
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
    pub fn new(
        tar_gz_bytes: Vec<u8>,
        config: PkgConfig,
        imports: Vec<Import>,
    ) -> Self {
        ComputeRequest {
            package: tar_gz_bytes,
            config,
            stdout: false,
            stderr: false,
            files: HashSet::new(),
            imports,
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

#[cfg(test)]
mod test {
    use super::*;
    use std::path::PathBuf;
    use tempdir::TempDir;
    use flate2::read::GzDecoder;
    use tar::Archive;

    fn unpack_targz(bytes: &[u8]) -> TempDir {
        use rand::Rng;
        let id: u32 = rand::thread_rng().gen();
        let tar = GzDecoder::new(bytes);
        let mut archive = Archive::new(tar);
        let root = TempDir::new(&id.to_string()).unwrap();
        if let Err(e) = archive.unpack(root.path()) {
            assert!(false, format!("malformed archive: {:?}", e));
        }
        root
    }

    #[test]
    fn compute_request_builder_basic() {
        let import = Import::Local {
            name: "numpy".to_string(),
            hash: "abc123".to_string(),
        };
        let builder = ComputeRequestBuilder::new("python")
            .args(vec!["run.py", "10"])
            .envs(vec!["VAR1=1", "VAR2=abc"])
            .import(import.clone());
        let request = match builder.finalize() {
            Ok(request) => request,
            Err(e) => {
                assert!(false, format!("{:?}", e));
                unreachable!()
            },
        };
        let config = request.config;
        assert_eq!(config.binary_path, Some(PathBuf::from("python")));
        assert!(config.mapped_dirs.is_empty());
        assert_eq!(config.args, vec!["run.py", "10"]);
        assert_eq!(config.envs, vec!["VAR1=1", "VAR2=abc"]);
        assert_eq!(request.imports, vec![import]);
    }

    #[test]
    fn compute_request_builder_add_file_simple() {
        let builder = ComputeRequestBuilder::new("python")
            .add_file("Cargo.toml");
        let request = match builder.finalize() {
            Ok(request) => request,
            Err(e) => {
                assert!(false, format!("{:?}", e));
                unreachable!()
            },
        };
        let root = unpack_targz(&request.package);
        assert!(root.path().join("Cargo.toml").exists());
        assert!(root.path().join("Cargo.toml").is_file());
    }

    #[test]
    fn compute_request_builder_add_file_layered() {
        let builder = ComputeRequestBuilder::new("node")
            .add_file("data/stt_node/weather.wav");
        let request = match builder.finalize() {
            Ok(request) => request,
            Err(e) => {
                assert!(false, format!("{:?}", e));
                unreachable!()
            },
        };
        let root = unpack_targz(&request.package);
        assert!(root.path().join("data").is_dir());
        assert!(root.path().join("data/stt_node").is_dir());
        assert!(root.path().join("data/stt_node/weather.wav").exists());
        assert!(root.path().join("data/stt_node/weather.wav").is_file());
    }

    #[test]
    fn compute_request_builder_add_dir_simple() {
        let builder = ComputeRequestBuilder::new("python")
            .add_dir("examples");
        let request = match builder.finalize() {
            Ok(request) => request,
            Err(e) => {
                assert!(false, format!("{:?}", e));
                unreachable!()
            },
        };
        let root = unpack_targz(&request.package);
        assert!(root.path().join("examples").is_dir());
        assert!(root.path().join("examples/stt_client.rs").exists());
        assert!(root.path().join("examples/stt_client.rs").is_file());
    }

    #[test]
    fn compute_request_builder_add_dir_layered() {
        let builder = ComputeRequestBuilder::new("python")
            .add_dir("data/add");
        let request = match builder.finalize() {
            Ok(request) => request,
            Err(e) => {
                assert!(false, format!("{:?}", e));
                unreachable!()
            },
        };
        let root = unpack_targz(&request.package);
        assert!(root.path().join("data").is_dir());
        assert!(root.path().join("data/add").is_dir());
        assert!(root.path().join("data/add/add.py").exists());
        assert!(root.path().join("data/add/add.py").is_file());
        assert!(root.path().join("data/add/lib").exists(), "run setup script?");
        assert!(root.path().join("data/add/lib").is_dir());
    }
}
