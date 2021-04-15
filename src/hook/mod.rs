use std::fs;
use std::path::{Path, PathBuf};
use bincode;
use serde::{Serialize, Deserialize};
use tokio::time::Duration;
use crate::common::*;
use crate::protos;

pub const HOOK_STORE_PATH: &str = "hooks";

pub type DomainName = String;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct FileACL {
    pub path: PathBuf,
    pub read: bool,
    pub write: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum HookSchedule {
    Interval(Duration),
    WatchFile(String),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Hook {
    confirmed: bool,
    pub global_hook_id: StringID,
    pub schedule: HookSchedule,
    pub state_perm: Vec<SensorID>,
    pub network_perm: Vec<DomainName>,
    pub file_perm: Vec<FileACL>,
    pub package: Vec<u8>,
    pub binary_path: PathBuf,
    pub args: Vec<String>,
    pub envs: Vec<(String, String)>,
}

impl FileACL {
    pub fn new(path: &str, read: bool, write: bool) -> Self {
        Self {
            path: Path::new(path).to_path_buf(),
            read,
            write,
        }
    }
}

impl From<&protos::FileAcl> for FileACL {
    fn from(acl: &protos::FileAcl) -> Self {
        Self {
            path: Path::new(&acl.path).to_path_buf(),
            read: acl.read,
            write: acl.write,
        }
    }
}

impl Into<protos::FileAcl> for FileACL {
    fn into(self) -> protos::FileAcl {
        protos::FileAcl {
            path: self.path.into_os_string().into_string().unwrap(),
            read: self.read,
            write: self.write,
        }
    }
}

impl Hook {
    pub fn new(
        global_hook_id: StringID,
        schedule: HookSchedule,
        state_perm: Vec<SensorID>,
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
            state_perm,
            network_perm,
            file_perm,
            package,
            binary_path,
            args,
            envs,
        }
    }

    pub fn import(global_hook_id: &StringID) -> Result<Self, Error> {
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

    pub fn set_state_perm(mut self, state_perm: Vec<SensorID>) -> Self {
        self.state_perm = state_perm;
        self
    }

    pub fn set_file_perm(mut self, file_perm: Vec<FileACL>) -> Self {
        self.file_perm = file_perm;
        self
    }

    /// `<KEY>=<VALUE>`
    pub fn set_envs(mut self, envs: Vec<String>) -> Result<Self, Error> {
        self.envs = vec![];
        for env in envs {
            let env = env.split("=").collect::<Vec<_>>();
            if env.len() != 2 {
                return Err(Error::HookInstallError(format!(
                    "bad format for envvar: {:?}", env)));
            }
            self.envs.push((env[0].to_string(), env[1].to_string()));
        }
        Ok(self)
    }

    pub fn confirm(&mut self) {
        self.confirmed = true;
    }

    /// Converts the hook to a protobuf compute request.
    ///
    /// The caller must set the request token before sending the compute
    /// reuqest to a host over the network.
    pub fn to_compute_request(&self) -> Result<protos::ComputeRequest, Error> {
        let mut req = protos::ComputeRequest::default();
        let hook = self.clone();
        req.package = hook.package;
        req.binary_path = hook.binary_path.into_os_string().into_string().unwrap();
        req.args = hook.args.into_iter().collect();
        req.envs = hook.envs.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
        req.file_perm = hook.file_perm.into_iter().map(|acl| acl.into()).collect();
        req.state_perm = hook.state_perm.into_iter().collect();
        req.network_perm = hook.network_perm.into_iter().collect();
        Ok(req)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_to_compute_request_works() {
        let package = vec![0, 1, 2, 3];
        let binary_path = "binary_path";
        let args = vec!["arg1".to_string(), "arg2".to_string()];
        let envs = vec![("KEY".to_string(), "VALUE".to_string())];
        let state_perm = vec!["camera".to_string()];
        let network_perm = vec!["https://www.stanford.edu".to_string()];
        let file_perm = vec![FileACL::new("main", true, true)];

        let hook = Hook::new(
            "hook_id".to_string(),
            HookSchedule::Interval(Duration::from_secs(10)),
            state_perm.clone(),
            network_perm.clone(),
            file_perm.clone(),
            package.clone(),
            binary_path,
            args.clone(),
            envs.clone(),
        );
        let r = hook.to_compute_request().unwrap();
        assert_eq!(r.package, package);
        assert_eq!(r.binary_path, binary_path);
        assert_eq!(r.args, args);
        let expected_envs: Vec<_> =
            envs.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
        assert_eq!(r.envs, expected_envs);
        assert_eq!(r.state_perm, state_perm);
        assert_eq!(r.network_perm, network_perm);
        assert_eq!(r.file_perm.len(), 1);
        assert_eq!(FileACL::from(&r.file_perm[0]), file_perm[0]);
    }

    #[test]
    #[ignore]
    fn test_hooks_can_be_deserialized() {
        Hook::import(&"person-detection".to_string()).unwrap();
        Hook::import(&"speech-to-text".to_string()).unwrap();
        Hook::import(&"bulb-intensity".to_string()).unwrap();
        Hook::import(&"announcement".to_string()).unwrap();
        Hook::import(&"livestream".to_string()).unwrap();
        Hook::import(&"firmware-update".to_string()).unwrap();
        Hook::import(&"search-engine".to_string()).unwrap();
        Hook::import(&"bug-report".to_string()).unwrap();
        Hook::import(&"bulb-integration".to_string()).unwrap();
    }
}
