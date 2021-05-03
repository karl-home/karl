use std::fs;
use std::path::Path;

use tar::Builder;
use flate2::{Compression, write::GzEncoder};

use super::Error;

/// Package builder, where a package is the input filesystem in a targz.
#[derive(Default)]
pub struct TarBuilder {
    dirs: Vec<(String, String)>,
    files: Vec<(String, String)>,
}

impl TarBuilder {
    pub fn new() -> TarBuilder {
        Default::default()
    }

    /// Add a file to the input root from the home filesystem, overwriting
    /// files with the same name.
    pub fn add_file_as(mut self, src_path: &str, dst_path: &str) -> TarBuilder {
        self.files.push((src_path.to_string(), dst_path.to_string()));
        self
    }

    /// Same as `add_file_as`, but uses the src path as the dst path.
    pub fn add_file(mut self, src_path: &str) -> TarBuilder {
        self.files.push((src_path.to_string(), src_path.to_string()));
        self
    }

    /// Add a directory to the input root from the home filesystem, overwriting
    /// files with the same name.
    ///
    /// TODO: needs to be a relative path. should be forward searching only.
    pub fn add_dir_as(mut self, src_path: &str, dst_path: &str) -> TarBuilder {
        self.dirs.push((src_path.to_string(), dst_path.to_string()));
        self
    }

    /// Same as `add_dir_as`, but uses the src path as the dst path.
    pub fn add_dir(mut self, src_path: &str) -> TarBuilder {
        self.dirs.push((src_path.to_string(), src_path.to_string()));
        self
    }

    /// Finalize the compute request.
    pub fn finalize(self) -> Result<Vec<u8>, Error> {
        // Tar up the input root.
        let mut buffer = Vec::new();
        let enc = GzEncoder::new(&mut buffer, Compression::default());
        let mut tar = Builder::new(enc);
        for (src_path, dst_path) in &self.dirs {
            tar.append_dir_all(dst_path, src_path)?;
        }
        for (src_path, dst_path) in &self.files {
            let src_parent = Path::new(src_path).parent().unwrap();
            let dst_parent = Path::new(dst_path).parent().unwrap();
            if dst_parent.parent().is_some() {
                tar.append_dir(dst_parent, src_parent)?;
            }
            tar.append_file(dst_path, &mut fs::File::open(src_path)?)?;
        }
        drop(tar);
        Ok(buffer)
    }
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
    fn tar_builder_add_file_simple() {
        let builder = TarBuilder::new()
            .add_file("Cargo.toml");
        let targz = match builder.finalize() {
            Ok(request) => request,
            Err(e) => {
                assert!(false, format!("{:?}", e));
                unreachable!()
            },
        };
        let root = unpack_targz(&targz);
        assert!(root.path().join("Cargo.toml").exists());
        assert!(root.path().join("Cargo.toml").is_file());
    }

    #[test]
    fn tar_builder_add_file_layered() {
        let builder = TarBuilder::new()
            .add_file("data/stt_node/weather.wav");
        let targz = match builder.finalize() {
            Ok(request) => request,
            Err(e) => {
                assert!(false, format!("{:?}", e));
                unreachable!()
            },
        };
        let root = unpack_targz(&targz);
        assert!(root.path().join("data").is_dir());
        assert!(root.path().join("data/stt_node").is_dir());
        assert!(root.path().join("data/stt_node/weather.wav").exists());
        assert!(root.path().join("data/stt_node/weather.wav").is_file());
    }

    #[test]
    fn tar_builder_add_dir_simple() {
        let builder = TarBuilder::new()
            .add_dir("examples");
        let targz = match builder.finalize() {
            Ok(request) => request,
            Err(e) => {
                assert!(false, format!("{:?}", e));
                unreachable!()
            },
        };
        let root = unpack_targz(&targz);
        assert!(root.path().join("examples").is_dir());
        assert!(root.path().join("examples/hello_world.rs").exists());
        assert!(root.path().join("examples/hello_world.rs").is_file());
    }

    #[test]
    fn tar_builder_add_dir_layered() {
        let builder = TarBuilder::new()
            .add_dir("data/stt");
        let targz = match builder.finalize() {
            Ok(request) => request,
            Err(e) => {
                assert!(false, format!("{:?}", e));
                unreachable!()
            },
        };
        let root = unpack_targz(&targz);
        assert!(root.path().join("data").is_dir());
        assert!(root.path().join("data/stt").is_dir());
        assert!(root.path().join("data/stt/client.py").exists());
        assert!(root.path().join("data/stt/client.py").is_file());
        assert!(root.path().join("data/stt/audio").exists(), "run setup script?");
        assert!(root.path().join("data/stt/audio").is_dir());
    }
}
