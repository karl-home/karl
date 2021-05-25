use std::fs;
use std::path::PathBuf;
use std::time::Instant;

#[cfg(target_os = "linux")]
use sys_mount::{SupportedFilesystems, Mount, MountFlags, Unmount, UnmountFlags};

use flate2::read::GzDecoder;
use tar::Archive;
use karl_common::*;

pub struct RequestPath {
    pub request_path: PathBuf,
    pub root_path: PathBuf,
}

pub struct PathManager {
    /// .karl/<host_id>
    host_path: PathBuf,
    /// .karl/cache
    cache_path: PathBuf,
}

impl Drop for PathManager {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.host_path);
    }
}

impl PathManager {
    /// Creates paths that do not already exist.
    pub fn new(base_path: PathBuf, id: u32) -> Self {
        #[cfg(target_os = "linux")]
        {
            assert!(SupportedFilesystems::new().unwrap().is_supported("aufs"));
        }
        #[cfg(not(target_os = "linux"))]
        {
            warn!("sys_mount is not supported on non-linux distributions");
        }
        let host_path = base_path.join(format!("host-{}", id));
        let cache_path = base_path.join("cache");
        fs::create_dir_all(&host_path).unwrap();
        fs::create_dir_all(&cache_path).unwrap();
        Self {
            host_path,
            cache_path,
        }
    }

    /// Create a new directory for a request.
    #[cfg(target_os = "linux")]
    pub fn new_request(
        &self,
        module_id: &str,
    ) -> Result<(Mount, RequestPath), Error> {
        use rand::Rng;
        let random: u32 = rand::thread_rng().gen();
        let request_path = self.host_path.join(random.to_string());
        let module_path = self.get_module_path(module_id);
        let root_path = request_path.join("root");
        let work_path = request_path.join("work");
        fs::create_dir_all(&root_path).expect("error writing to request path");
        fs::create_dir_all(&work_path).expect("error writing to request path");

        let fstype = "aufs";
        let options = format!(
            "br={}:{}=ro",
            work_path.to_str().unwrap(),
            module_path.to_str().unwrap(),
        );
        debug!("mounting to {:?} fstype={:?} options={:?}", root_path, fstype, options);
        let mount = Mount::new(
            "none",
            &root_path,
            fstype,
            MountFlags::empty(),
            Some(&options),
        )?;
        Ok((mount, RequestPath { request_path, root_path }))
    }

    #[cfg(target_os = "linux")]
    pub fn unmount(&self, mount: Mount) -> Result<(), Error> {
        // Note that a filesystem cannot be unmounted when it is 'busy' - for
        // example, when there are open files on it, or when some process has
        // its working directory there, or when a swap file on it is in use.
        mount.unmount(UnmountFlags::DETACH)?;
        Ok(())
    }

    /********************** COLD CACHE FUNCTIONALITY **********************/
    /// Returns path to cached module.
    pub fn get_module_path(&self, module_id: &str) -> PathBuf {
        self.cache_path.join(module_id)
    }

    pub fn is_cached(&self, module_id: &str) -> bool {
        self.get_module_path(module_id).is_dir()
    }

    /// Unpackage the bytes of the tarred and gzipped request to the cache.
    /// Returns false if the module already existed, so it didn't do anything.
    pub fn cache_module(
        &self,
        module_id: &str,
        package: Vec<u8>,
    ) -> Result<bool, Error> {
        if self.is_cached(module_id) {
            return Ok(false);
        }
        let now = Instant::now();
        let tar = GzDecoder::new(&package[..]);
        let mut archive = Archive::new(tar);
        let path = self.get_module_path(module_id);
        fs::create_dir_all(&path).unwrap();
        archive.unpack(&path).map_err(|e| format!("malformed tar.gz: {:?}", e))?;
        info!("=> cached module at {:?}: {} s", &path, now.elapsed().as_secs_f32());
        Ok(true)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use tempdir::TempDir;

    fn init_test() -> (TempDir, PathManager) {
        let dir = TempDir::new("karl").unwrap();
        let cache = PathManager::new(dir.path().to_path_buf(), 777);
        (dir, cache)
    }

    #[test]
    fn test_init_path_manager() {
        let (dir, cache) = init_test();
        let base_path = dir.path();
        assert!(base_path.is_dir());
        let host_path = base_path.join("host-777");
        let cache_path = base_path.join("cache");
        assert_eq!(cache.host_path, host_path);
        assert_eq!(cache.cache_path, cache_path);
        assert!(host_path.is_dir(), "host path was created");
        assert!(cache_path.is_dir(), "cache path was created");
    }

    #[test]
    fn test_drop_path_manager() {
        let (dir, cache) = init_test();
        let base_path = dir.path();
        let host_path = cache.host_path.clone();
        let cache_path = cache.cache_path.clone();
        drop(cache);
        assert!(base_path.exists());
        assert!(!host_path.exists(), "host path was dropped with path manager");
        assert!(cache_path.exists(), "cache path stays after drop");
    }

    #[test]
    fn test_uncached_module_path() {
        let (dir, cache) = init_test();
        let module_id = "person_detection";
        let module_path = dir.path().join("cache").join(module_id);
        assert_eq!(module_path, cache.get_module_path(module_id));
        assert!(!module_path.exists());
        assert!(!cache.is_cached(module_id));
        fs::write(&module_path, vec![5, 5, 5, 5]).unwrap();
        assert!(module_path.exists());
        assert!(!cache.is_cached(module_id), "still not cached because it is a file");
    }

    #[test]
    fn test_new_request_uncached_module() {
        let (_dir, cache) = init_test();
        let module_id = "person_detection";
        assert!(cache.new_request(module_id).is_err(), "cached module does not exist");
        fs::write(cache.get_module_path(module_id), vec![5, 5, 5, 5]).unwrap();
        assert!(cache.new_request(module_id).is_err(), "cached module is a file");
    }

    // TODO: requires sudo
    #[test]
    #[ignore]
    fn test_new_request_cached_module_paths() {
        let (_dir, cache) = init_test();
        let module_id = "person_detection";
        let module_path = cache.get_module_path(module_id);
        fs::create_dir_all(&module_path).unwrap();
        fs::write(module_path.join("myfile1"), vec![5, 5, 5, 5]).unwrap();

        let (_mount, paths) = {
            let result = cache.new_request(module_id);
            assert!(result.is_ok(), "successfully mounted aufs");
            result.unwrap()
        };

        assert_eq!(paths.request_path.parent(), Some(cache.host_path.as_path()));
        assert_eq!(paths.root_path.parent(), Some(paths.request_path.as_path()));
        assert!(paths.root_path.join("myfile1").exists(), "file was mounted");
    }

    // TODO: requires sudo
    #[test]
    #[ignore]
    fn test_new_request_overlay_filesystem() {
        let (_dir, cache) = init_test();
        let module_id = "person_detection";
        let module_path = cache.get_module_path(module_id);
        fs::create_dir_all(&module_path).unwrap();
        fs::write(module_path.join("myfile1"), vec![5, 5, 5, 5]).unwrap();

        let (_mount, paths) = {
            let result = cache.new_request(module_id);
            assert!(result.is_ok(), "successfully mounted aufs");
            result.unwrap()
        };

        assert!(fs::write(paths.root_path.join("myfile2"), vec![4, 4]).is_ok(),
            "successfully wrote file to request root path");
        assert!(fs::remove_file(paths.root_path.join("myfile1")).is_ok(),
            "successfully removed file from request root path");
        assert!(paths.root_path.join("myfile2").exists(), "successful write");
        assert!(!paths.root_path.join("myfile1").exists(), "successful remove");
        assert!(!module_path.join("myfile2").exists(), "readonly module path");
        assert!(module_path.join("myfile1").exists(), "readonly module path");
    }

    // TODO: requires sudo
    #[test]
    #[ignore]
    fn test_unmount_paths() {
        let (_dir, cache) = init_test();
        let module_id = "person_detection";
        let module_path = cache.get_module_path(module_id);
        fs::create_dir_all(&module_path).unwrap();
        fs::write(module_path.join("myfile1"), vec![5, 5, 5, 5]).unwrap();

        let (mount, paths) = cache.new_request(module_id).unwrap();
        assert!(paths.root_path.join("myfile1").exists());
        assert!(cache.unmount(mount).is_ok());
        assert!(!paths.root_path.join("myfile1").exists());
    }

    #[test]
    fn test_cache_module_malformed() {
        let (_dir, cache) = init_test();
        let module_id = "person_detection";
        assert!(cache.cache_module(module_id, vec![5, 5, 5, 5]).is_err())
    }

    #[test]
    fn test_cache_module_already_exists() {
        let (_dir, cache) = init_test();
        let module_id = "person_detection";
        let module_path = cache.get_module_path(module_id);
        fs::create_dir_all(&module_path).unwrap();
        let result = cache.cache_module(module_id, vec![5, 5, 5, 5]);
        assert!(result.is_ok(), "cached module already exists");
        assert!(!result.unwrap(), "and it was not overwritten");
    }

    #[test]
    fn test_cache_module_new() {
        let (_dir, cache) = init_test();
        let module_id = "person_detection";
        let module_path = cache.get_module_path(module_id);
        let targz = vec![
            31, 139, 8, 0, 0, 0, 0, 0, 0, 3, 237, 206, 49, 14, 131, 48, 16, 4,
            192, 123, 138, 159, 96, 7, 108, 222, 227, 42, 162, 73, 17, 200, 255,
            3, 5, 18, 85, 82, 65, 53, 35, 157, 182, 216, 45, 174, 199, 245, 242,
            166, 181, 113, 207, 50, 213, 124, 206, 67, 148, 49, 215, 161, 108,
            55, 236, 187, 169, 61, 106, 164, 124, 195, 111, 241, 89, 214, 254,
            78, 41, 158, 243, 171, 255, 218, 253, 235, 1, 0, 0, 0, 0, 0, 0, 0,
            0, 224, 70, 95, 9, 133, 139, 211, 0, 40, 0, 0,
        ];
        let result = cache.cache_module(module_id, targz);
        assert!(result.is_ok());
        assert!(result.unwrap(), "new module was created");
        assert!(cache.is_cached(module_id));
        assert!(module_path.exists());
        assert!(module_path.join("a").exists(),
            "files inside the targz were unpacked");
    }
}
