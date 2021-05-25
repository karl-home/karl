use std::fs;
use std::path::{Path, PathBuf};
use bincode;
use serde::{Serialize, Deserialize};
use crate::*;

type DomainName = String;

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Module {
    pub global_id: GlobalModuleID,
    pub package: Vec<u8>,
    pub binary_path: PathBuf,
    pub args: Vec<String>,
    pub params: Vec<String>,
    pub returns: Vec<String>,
    pub network_perm: Vec<DomainName>,
}

impl Module {
    pub fn import<S: AsRef<Path>>(global_id: S) -> Result<Self, Error> {
        let modules_path = std::env::var("KARL_MODULE_PATH")
            .unwrap_or("../modules".to_string());
        let path = Path::new(&modules_path).join(global_id);
        let bytes = fs::read(path)?;
        let module: Module = bincode::deserialize(&bytes[..])
            .map_err(|e| Error::ModuleInstallError(e.to_string()))?;
        Ok(module)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_bad_module_import() {
        assert!(Module::import("NONEXISTENT").is_err(), "path does not exist");
    }

    #[test]
    fn test_static_modules_can_be_deserialized() {
        assert!(Module::import("differential_privacy").is_ok());
        assert!(Module::import("false").is_ok());
        assert!(Module::import("firmware_update").is_ok());
        assert!(Module::import("light_switch").is_ok());
        assert!(Module::import("search").is_ok());
        assert!(Module::import("targz").is_ok());
        assert!(Module::import("true").is_ok());
    }

    #[test]
    #[ignore]
    fn test_command_classifier_module_can_be_deserialized() {
        // takes a long time
        assert!(Module::import("command_classifier").is_ok());
    }

    #[test]
    #[ignore]
    fn test_person_detection_module_can_be_deserialized() {
        // takes a long time
        assert!(Module::import("person_detection").is_ok());
    }
}
