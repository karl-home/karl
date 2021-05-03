use std::collections::{HashSet, HashMap};
use std::path::{Path, PathBuf};

use crate::protos::*;
use karl_common::*;

/// Permissions of an active process
#[derive(Debug)]
pub struct ProcessPerms {
    /// State change permissions, tag to state key
    state_perms: HashMap<String, String>,
    /// Network permissions
    network_perms: HashSet<String>,
    /// File read permissions
    read_perms: HashSet<PathBuf>,
    /// File write permissions
    write_perms: HashSet<PathBuf>,
}

impl ProcessPerms {
    pub fn new(req: &ComputeRequest) -> Self {
        let state_perms: HashMap<_, _> = req.state_perm.clone().into_iter()
            .filter_map(|perm| {
                let mut split = perm.split("=");
                if let Some(tag) = split.next() {
                    if let Some(key) = split.next() {
                        return Some((tag.to_string(), key.to_string()));
                    }
                }
                None
            })
            .collect();
        let network_perms: HashSet<_> = req.network_perm.clone().into_iter().collect();
        let read_perms: HashSet<_> = req.file_perm.iter()
            .filter(|acl| acl.read)
            .map(|acl| Path::new(&sanitize_path(&acl.path)).to_path_buf())
            .collect();
        // Processes cannot write to raw/
        let write_perms: HashSet<_> = req.file_perm.iter()
            .filter(|acl| acl.write)
            .map(|acl| sanitize_path(&acl.path))
            .filter(|path| !(path == "raw" || path.starts_with("raw/")))
            .map(|path| Path::new(&path).to_path_buf())
            .collect();
        Self { state_perms, network_perms, read_perms, write_perms }
    }

    pub fn can_change_state(&self, tag: &str) -> Option<&String> {
        self.state_perms.get(tag)
    }

    pub fn can_access_domain(&self, domain: &str) -> bool {
        self.network_perms.contains(domain)
    }

    pub fn _can_read_file(&self, path: &Path) -> bool {
        let mut next_path = Some(path);
        while let Some(path) = next_path {
            if self.read_perms.contains(path) {
                return true;
            }
            next_path = path.parent();
        }
        false
    }

    pub fn _can_write_file(&self, path: &Path) -> bool {
        let mut next_path = Some(path);
        while let Some(path) = next_path {
            if self.write_perms.contains(path) {
                return true;
            }
            next_path = path.parent();
        }
        false
    }
}
