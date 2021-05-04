use std::fs;
use std::path::{Path, PathBuf};
use bincode;
use itertools::Itertools;
use serde::{Serialize, Deserialize};
use std::time::Duration;
use std::collections::HashMap;
use crate::*;

type DomainName = String;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct FileACL {
    pub path: PathBuf,
    pub read: bool,
    pub write: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum HookSchedule {
    Interval(Duration),
    WatchTag(String),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Hook {
    confirmed: bool,
    /// The name of the module in the global repository.
    pub global_hook_id: StringID,
    /// The targz of the input filesystem.
    pub package: Vec<u8>,
    /// Path to the binary within the input filesystem.
    pub binary_path: PathBuf,
    /// Arguments to the binary path.
    pub args: Vec<String>,
    /// Map from the module's expected parameters and the associated tags.
    pub params: HashMap<String, Option<String>>,
    /// Map from the module's expected return names and the associated tags.
    pub returns: HashMap<String, Vec<String>>,
    pub network_perm: Vec<DomainName>,
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

impl Hook {
    pub fn new(
        global_hook_id: StringID,
        package: Vec<u8>,
        binary_path: &str,
        args: Vec<String>,
        params: Vec<String>,
        returns: Vec<String>,
    ) -> Self {
        let binary_path = Path::new(binary_path).to_path_buf();
        Self {
            confirmed: false,
            global_hook_id,
            package,
            binary_path,
            args,
            params: params.into_iter().map(|p| (p, None)).collect(),
            returns: returns.into_iter().map(|r| (r, vec![])).collect(),
            md: Default::default(),
        }
    }

    pub fn import(global_hook_id: &StringID) -> Result<Self, Error> {
        let path = Path::new(HOOK_STORE_PATH).join(global_hook_id);
        let bytes = fs::read(path)?;
        let mut hook: Hook = bincode::deserialize(&bytes[..])
            .map_err(|e| Error::HookInstallError(e.to_string()))?;
        hook.confirm(); // TODO
        Ok(hook)
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

    pub fn params_string(&self) -> String {
        self.params.iter()
            .filter(|(_, tag)| tag.is_some())
            .map(|(param, tag)| format!("{};{}", param, tag.as_ref().unwrap()))
            .join(":")
    }

    pub fn returns_string(&self) -> String {
        self.returns.iter()
            .filter(|(_, tags)| !tags.is_empty())
            .map(|(ret, tags)| format!("{};{}", ret, tags.iter().join(",")))
            .join(":")
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
