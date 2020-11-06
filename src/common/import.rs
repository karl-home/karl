/*
use std::path::{Path, PathBuf};
use serde::{Serialize, Deserialize};
use wapm_cli::commands;

use super::Error;

/// External import.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
#[repr(C)]
pub enum Import {
    /// Wasm package manager
    Wapm {
        /// Package name
        name: String,
        /// Package version
        version: String,
    },
    /// A directory at `~/.karl/local/`.
    ///
    /// Local imports cannot be installed. They must already exist.
    Local {
        /// Directory name relative to `~/.karl/local`.
        name: String,
        /// Hash of the directory.
        hash: String,
    }
}

impl Import {
    /// Install the import to the given path if it does not already exist.
    ///
    /// Return the import path.
    pub fn install_if_missing(
        &self,
        karl_path: &Path,
    ) -> Result<PathBuf, Error> {
        // TODO: concurrent installs?
        let path = self.path(karl_path);
        // TODO: check hash of path not just if it exists and is not empty.
        if path.exists() && path.is_dir() {
            if path.is_dir() && path.read_dir().unwrap().next().is_none() {
                // empty
            } else {
                return Ok(path);
            }
        }
        match self {
            Import::Wapm { name, version } => {
                let opt = commands::InstallOpt::new_from_package(name, version);
                // Wapm installs packages relative to the current directory
                // TODO: reinstate previous working directory?
                std::env::set_current_dir(karl_path).unwrap();
                commands::install(opt).map_err(|e|
                    Error::InstallImportError(format!("{}", e)))?;
                warn!("installed wapm package {}@{}", name, version);
            },
            Import::Local { name, hash: _hash } => {
                return Err(Error::InstallImportError(format!(
                    "cannot install a local import: {}", name)));
            },
        }
        Ok(path)
    }

    fn path(&self, karl_path: &Path) -> PathBuf {
        match self {
            Import::Wapm { name, version } => {
                let path = format!("wapm_packages/_/{}@{}/", name, version);
                let path = karl_path.join(path);
                path
            },
            Import::Local { name, .. } => {
                let path = format!("local/{}/", name);
                let path = karl_path.join(path);
                path
            }
        }
    }
}
*/