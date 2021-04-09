use std::fs;
use std::time::Duration;
use std::path::{Path, PathBuf};
use bincode;
use serde::{Serialize, Deserialize};
use crate::common::{StringID, Error};

pub const HOOK_STORE_PATH: &str = "hooks";

pub type DomainName = String;

#[derive(Debug, Serialize, Deserialize)]
pub struct FileACL {
    pub path: PathBuf,
    pub read: bool,
    pub write: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum HookSchedule {
    Interval(Duration),
    WatchFile(PathBuf),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Hook {
    confirmed: bool,
    pub global_hook_id: StringID,
    pub schedule: HookSchedule,
    pub network_perm: Vec<DomainName>,
    pub file_perm: Vec<FileACL>,
    pub package: Vec<u8>,
    pub binary_path: PathBuf,
    pub args: Vec<String>,
    pub envs: Vec<(String, String)>,
}

impl Hook {
    pub fn new(
        global_hook_id: StringID,
        schedule: HookSchedule,
        network_perm: Vec<DomainName>,
        file_perm: Vec<FileACL>,
        package: Vec<u8>,
        binary_path: &str,
        args: Vec<String>,
        envs: Vec<(String, String)>,
    ) -> Self {
        let binary_path = Path::new(binary_path).to_path_buf();
        Self {
            confirmed: false,
            global_hook_id,
            schedule,
            network_perm,
            file_perm,
            package,
            binary_path,
            args,
            envs,
        }
    }

    pub fn import(global_hook_id: StringID) -> Result<Self, Error> {
        let path = Path::new(HOOK_STORE_PATH).join(global_hook_id);
        let bytes = fs::read(path)?;
        debug!("read {} bytes", bytes.len());
        let mut hook: Hook = bincode::deserialize(&bytes[..])
            .map_err(|e| Error::HookInstallError(e.to_string()))?;
        hook.confirm(); // TODO
        Ok(hook)
    }

    pub fn set_network_perm(mut self, network_perm: Vec<DomainName>) -> Self {
        self.network_perm = network_perm;
        self
    }

    pub fn set_file_perm(mut self, file_perm: Vec<FileACL>) -> Self {
        self.file_perm = file_perm;
        self
    }

    pub fn set_envs(mut self, envs: Vec<(String, String)>) -> Self {
        self.envs = envs;
        self
    }

    pub fn confirm(&mut self) {
        self.confirmed = true;
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_hooks_can_be_deserialized() {
        Hook::import("person-detection".to_string()).unwrap();
        Hook::import("speech-to-text".to_string()).unwrap();
        Hook::import("bulb-intensity".to_string()).unwrap();
        Hook::import("announcement".to_string()).unwrap();
        Hook::import("livestream".to_string()).unwrap();
        Hook::import("firmware-update".to_string()).unwrap();
        Hook::import("search-engine".to_string()).unwrap();
        Hook::import("bug-report".to_string()).unwrap();
        Hook::import("bulb-integration".to_string()).unwrap();
    }
}
