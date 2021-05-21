use std::fs;
use std::path::{Path, PathBuf};
use bincode;
use serde::{Serialize, Deserialize};
use crate::*;

type DomainName = String;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Module {
    pub global_id: StringID,
    pub package: Vec<u8>,
    pub binary_path: PathBuf,
    pub args: Vec<String>,
    pub params: Vec<String>,
    pub returns: Vec<String>,
    pub network_perm: Vec<DomainName>,
}

impl Module {
    pub fn import(global_id: &StringID) -> Result<Self, Error> {
        let modules_path = std::env::var("KARL_MODULE_PATH").unwrap();
        let path = Path::new(&modules_path).join(global_id);
        let bytes = fs::read(path)?;
        let hook: Module = bincode::deserialize(&bytes[..])
            .map_err(|e| Error::HookInstallError(e.to_string()))?;
        Ok(hook)
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

        let hook = Module::new(
            "hook_id".to_string(),
            HookSchedule::Interval(Duration::from_secs(10)),
            state_perm.clone(),
            network_perm.clone(),
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
