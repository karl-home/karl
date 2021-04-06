use std::io::Read;
use std::fs::File;
use std::time::SystemTime;
use std::path::{Path, PathBuf};
use bincode;
use serde::{Serialize, Deserialize};
use crate::common::{StringID, Error};

const HOOK_STORE_PATH: &str = "hooks";

pub type DomainName = String;

#[derive(Debug, Serialize, Deserialize)]
pub struct FileACL {
    pub path: PathBuf,
    pub read: bool,
    pub write: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum HookSchedule {
    Daily(SystemTime),
    WatchFile(PathBuf),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Hook {
    confirmed: bool,
    pub global_hook_id: String,
    pub schedule: HookSchedule,
    pub network_perm: Vec<DomainName>,
    pub file_perm: Vec<FileACL>,
    pub package: Vec<u8>,
    pub binary_path: PathBuf,
    pub args: Vec<String>,
    pub envs: Vec<(String, String)>,
}

impl Hook {
    pub fn new(global_hook_id: StringID) -> Result<Self, Error> {
        let path = Path::new(HOOK_STORE_PATH).join(global_hook_id);
        let mut bytes = vec![];
        let mut f = File::open(path)?;
        f.read(&mut bytes)?;
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
        assert!(Hook::new("person-detection".to_string()).is_ok());
        assert!(Hook::new("speech-to-text".to_string()).is_ok());
        assert!(Hook::new("bulb-intensity".to_string()).is_ok());
        assert!(Hook::new("announcement".to_string()).is_ok());
        assert!(Hook::new("livestream".to_string()).is_ok());
        assert!(Hook::new("firmware-update".to_string()).is_ok());
        assert!(Hook::new("search-engine".to_string()).is_ok());
        assert!(Hook::new("bug-report".to_string()).is_ok());
        assert!(Hook::new("bulb-integration".to_string()).is_ok());
    }
}
