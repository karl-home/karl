use std::fs;
use std::path::{Path, PathBuf};

use tar::Builder;
use flate2::{Compression, write::GzEncoder};

use crate::protos;
use super::Error;

/// Compute request builder.
#[repr(C)]
#[derive(Debug)]
pub struct ComputeRequestBuilder {
    pub dirs: Vec<(String, String)>,
    pub files: Vec<(String, String)>,
    pub imports: Vec<protos::Import>,
    pub config: protos::PkgConfig,
}

impl ComputeRequestBuilder {
    pub fn new(binary_path: &str) -> ComputeRequestBuilder {
        let mut config = protos::PkgConfig::default();
        config.set_binary_path(binary_path.to_string());
        ComputeRequestBuilder {
            dirs: Vec::new(),
            files: Vec::new(),
            imports: Vec::new(),
            config,
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
    pub fn import(mut self, import: protos::Import) -> ComputeRequestBuilder {
        self.imports.push(import);
        self
    }

    /// Add a file to the input root from the home filesystem, overwriting
    /// files with the same name.
    pub fn add_file_as(mut self, src_path: &str, dst_path: &str) -> ComputeRequestBuilder {
        self.files.push((src_path.to_string(), dst_path.to_string()));
        self
    }

    /// Same as `add_file_as`, but uses the src path as the dst path.
    pub fn add_file(mut self, src_path: &str) -> ComputeRequestBuilder {
        self.files.push((src_path.to_string(), src_path.to_string()));
        self
    }

    /// Add a directory to the input root from the home filesystem, overwriting
    /// files with the same name.
    ///
    /// TODO: needs to be a relative path. should be forward searching only.
    pub fn add_dir_as(mut self, src_path: &str, dst_path: &str) -> ComputeRequestBuilder {
        self.dirs.push((src_path.to_string(), dst_path.to_string()));
        self
    }

    /// Same as `add_dir_as`, but uses the src path as the dst path.
    pub fn add_dir(mut self, src_path: &str) -> ComputeRequestBuilder {
        self.dirs.push((src_path.to_string(), src_path.to_string()));
        self
    }

    /// Finalize the compute request.
    pub fn finalize(self) -> Result<protos::ComputeRequest, Error> {
        // Tar up the input root.
        let mut buffer = Vec::new();
        let enc = GzEncoder::new(&mut buffer, Compression::default());
        let mut tar = Builder::new(enc);
        for (src_path, dst_path) in &self.dirs {
            trace!("append_dir_all({:?}, {:?})", dst_path, src_path);
            tar.append_dir_all(dst_path, src_path)?;
        }
        for (src_path, dst_path) in &self.files {
            let src_parent = Path::new(src_path).parent().unwrap();
            let dst_parent = Path::new(dst_path).parent().unwrap();
            trace!("append_dir({:?}, {:?})", dst_parent, src_parent);
            if dst_parent.parent().is_some() {
                tar.append_dir(dst_parent, src_parent)?;
            }
            trace!("append_file({:?}, {:?})", dst_path, src_path);
            tar.append_file(dst_path, &mut fs::File::open(src_path)?)?;
        }

        // Generate the default compute request.
        drop(tar);
        let mut req = protos::ComputeRequest::default();
        req.set_package(buffer);
        req.set_config(self.config);
        req.set_imports(protobuf::RepeatedField::from_vec(self.imports));
        Ok(req)
    }
}

/// Get the path to the local import.
pub fn import_path(import: &protos::Import, karl_path: &Path) -> PathBuf {
    let path = format!("local/{}/", import.get_name());
    let path = karl_path.join(path);
    path
}

#[cfg(test)]
mod test {
    use super::*;
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
        let builder = ComputeRequestBuilder::new("python")
            .args(vec!["run.py", "10"])
            .envs(vec!["VAR1=1", "VAR2=abc"])
            .import(protos::Import {
                name: "numpy".to_string(),
                hash: "abc123".to_string(),
                ..Default::default()
            });
        let request = match builder.finalize() {
            Ok(request) => request,
            Err(e) => {
                assert!(false, format!("{:?}", e));
                unreachable!()
            },
        };
        let config = request.get_config();
        assert_eq!(config.get_binary_path(), "python");
        assert_eq!(config.get_args().to_vec(), vec!["run.py", "10"]);
        assert_eq!(config.get_envs().to_vec(), vec!["VAR1=1", "VAR2=abc"]);
        assert_eq!(request.get_imports().len(), 1);
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
