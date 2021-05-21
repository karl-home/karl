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
    pub fn new(karl_path: PathBuf, id: u32) -> Self {
        #[cfg(target_os = "linux")]
        {
            assert!(SupportedFilesystems::new().unwrap().is_supported("aufs"));
        }
        #[cfg(not(target_os = "linux"))]
        {
            warn!("sys_mount is not supported on non-linux distributions");
        }
        let host_path = karl_path.join(id.to_string());
        let cache_path = karl_path.join("cache");
        fs::create_dir_all(&host_path).unwrap();
        fs::create_dir_all(&cache_path).unwrap();
        // Set the current working directory to the <KARL_PATH>.
        std::env::set_current_dir(&karl_path).unwrap();
        Self {
            host_path,
            cache_path,
        }
    }

    /// Create a new directory for a request.
    #[cfg(target_os = "linux")]
    pub fn new_request(&self, hook_id: &ModuleID) -> (Mount, RequestPath) {
        use rand::Rng;
        let random: u32 = rand::thread_rng().gen();
        let request_path = self.host_path.join(random.to_string());
        let hook_path = self.get_hook_path(hook_id);
        let root_path = request_path.join("root");
        let work_path = request_path.join("work");
        fs::create_dir_all(&root_path).unwrap();
        fs::create_dir_all(&work_path).unwrap();

        let fstype = "aufs";
        let options = format!(
            "br={}:{}=ro",
            work_path.to_str().unwrap(),
            hook_path.to_str().unwrap(),
        );
        debug!("mounting to {:?} fstype={:?} options={:?}", root_path, fstype, options);
        let mount = Mount::new(
            "none",
            &root_path,
            fstype,
            MountFlags::empty(),
            Some(&options),
        ).unwrap();
        (mount, RequestPath { request_path, root_path })
    }

    #[cfg(target_os = "linux")]
    pub fn unmount(&self, mount: Mount) {
        // Note that a filesystem cannot be unmounted when it is 'busy' - for
        // example, when there are open files on it, or when some process has
        // its working directory there, or when a swap file on it is in use.
        if let Err(e) = mount.unmount(UnmountFlags::DETACH) {
            error!("error unmounting: {:?}", e);
        }
    }

    /********************** COLD CACHE FUNCTIONALITY **********************/
    /// Returns path to cached hook.
    pub fn get_hook_path(&self, hook_id: &ModuleID) -> PathBuf {
        self.cache_path.join(hook_id)
    }

    pub fn is_cached(&self, hook_id: &ModuleID) -> bool {
        self.get_hook_path(hook_id).exists()
    }

    /// Unpackage the bytes of the tarred and gzipped request to the cache.
    /// Returns false if the hook already existed, so it didn't do anything.
    pub fn cache_hook(
        &self,
        hook_id: &ModuleID,
        package: Vec<u8>,
    ) -> Result<bool, Error> {
        let path = self.get_hook_path(hook_id);
        if path.exists() {
            return Ok(false);
        }
        let now = Instant::now();
        let tar = GzDecoder::new(&package[..]);
        let mut archive = Archive::new(tar);
        fs::create_dir_all(&path).unwrap();
        archive.unpack(&path).map_err(|e| format!("malformed tar.gz: {:?}", e))?;
        info!("=> cached hook at {:?}: {} s", &path, now.elapsed().as_secs_f32());
        Ok(true)
    }
}
