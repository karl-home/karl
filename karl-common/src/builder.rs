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
            assert!(false, "malformed archive: {:?}", e);
        }
        root
    }

    #[test]
    fn add_nonexistent_files_to_tar_builder() {
        assert!(TarBuilder::new().add_file("a").finalize().is_err());
        assert!(TarBuilder::new().add_file_as("a", "a").finalize().is_err());
        assert!(TarBuilder::new().add_dir("a").finalize().is_err());
        assert!(TarBuilder::new().add_dir_as("a", "a").finalize().is_err());
    }

    #[test]
    fn tar_builder_add_file_simple() {
        let builder = TarBuilder::new()
            .add_file("Cargo.toml");
        let targz = match builder.finalize() {
            Ok(request) => request,
            Err(e) => {
                assert!(false, "{:?}", e);
                unreachable!()
            },
        };
        let root = unpack_targz(&targz);
        assert!(root.path().join("Cargo.toml").exists());
        assert!(root.path().join("Cargo.toml").is_file());
    }

    #[test]
    fn tar_builder_add_file_as() {
        let builder = TarBuilder::new()
            .add_file_as("Cargo.toml", "filename");
        let targz = match builder.finalize() {
            Ok(request) => request,
            Err(e) => {
                assert!(false, "{:?}", e);
                unreachable!()
            },
        };
        let root = unpack_targz(&targz);
        assert!(!root.path().join("Cargo.toml").exists());
        assert!(root.path().join("filename").exists());
        assert!(root.path().join("filename").is_file());
    }

    #[test]
    fn tar_builder_add_file_layered() {
        let builder = TarBuilder::new()
            .add_file("src/bin/build_static.rs");
        let targz = match builder.finalize() {
            Ok(request) => request,
            Err(e) => {
                assert!(false, "{:?}", e);
                unreachable!()
            },
        };
        let root = unpack_targz(&targz);
        assert!(root.path().join("src").is_dir());
        assert!(root.path().join("src/bin").is_dir());
        assert!(root.path().join("src/bin/build_static.rs").exists());
        assert!(root.path().join("src/bin/build_static.rs").is_file());
    }

    #[test]
    fn tar_builder_add_dir_layered() {
        let builder = TarBuilder::new()
            .add_dir("src");
        let targz = match builder.finalize() {
            Ok(request) => request,
            Err(e) => {
                assert!(false, "{:?}", e);
                unreachable!()
            },
        };
        let root = unpack_targz(&targz);
        assert!(root.path().join("src").is_dir());
        assert!(root.path().join("src/lib.rs").exists());
        assert!(root.path().join("src/lib.rs").is_file());
        assert!(root.path().join("src/bin").exists());
        assert!(root.path().join("src/bin").is_dir());
    }

    #[test]
    fn tar_builder_add_dir_as() {
        let builder = TarBuilder::new()
            .add_dir_as("src", "dirname");
        let targz = match builder.finalize() {
            Ok(request) => request,
            Err(e) => {
                assert!(false, "{:?}", e);
                unreachable!()
            },
        };
        let root = unpack_targz(&targz);
        assert!(root.path().join("dirname").is_dir());
        assert!(root.path().join("dirname/lib.rs").exists());
        assert!(root.path().join("dirname/lib.rs").is_file());
        assert!(root.path().join("dirname/bin").exists());
        assert!(root.path().join("dirname/bin").is_dir());
    }
}
