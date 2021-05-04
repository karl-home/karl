use std::collections::{HashSet, HashMap};
use std::path::{Path, PathBuf};

use crate::protos::*;
use karl_common::*;

/// Permissions of an active process
#[derive(Debug)]
pub struct ProcessPerms {
    /// Triggered tag
    triggered_tag: String,
    triggered_timestamp: String,
    triggered_data: Option<Vec<u8>>,
    /// Tags the process can read from
    read_perms: HashSet<String>,
    /// Tags the process can write to
    write_perms: HashSet<String>,
    /// Domains allowed to contact
    network_perms: HashSet<String>,
}

impl ProcessPerms {
    pub fn new(req: &ComputeRequest) -> Self {
        let triggered_data = if req.triggered_tag == "" || req.triggered_timestamp == "" {
            None
        } else {
            Some(req.triggered_data)
        };
        let read_perms = req.params
            .split(":")
            .map(|param| param.split(";"))
            .map(|param| (param.next().unwrap(), param.next().unwrap()))
            .map(|(_, tag)| tag.to_string())
            .filter(|tag| tag != req.triggered_tag)
            .collect::HashSet<_>();
        let write_perms = req.returns
            .split(":")
            .map(|param| param.split(";"))
            .map(|param| (param.next().unwrap(), param.next().unwrap()))
            .flat_map(|(_, tags)| tags.split(",").map(|tag| tag.to_string()).collect())
            .collect::HashSet<_>();
        let network_perms: HashSet<_> = req.network_perm.clone().into_iter().collect();
        Self {
            triggered_tag: req.triggered_tag,
            triggered_timestamp: req.triggered_timestamp,
            triggered_data,
            read_perms,
            write_perms,
            network_perms,
        }
    }

    pub fn is_triggered(&self, tag: &str) -> bool {
        self.triggered_tag == tag
    }

    pub fn read_triggered(&mut self, timestamp: &str) -> Option<Vec<u8>> {
        if self.triggered_timestamp == timestamp {
            self.triggered_data.take()
        } else {
            None
        }
    }

    pub fn can_access_domain(&self, domain: &str) -> bool {
        self.network_perms.contains(domain)
    }

    pub fn can_read(&self, tag: &str) -> bool {
        self.read_perms.contains(tag)
    }

    pub fn can_write(&self, path: &Path) -> bool {
        self.write_perms.contains(tag)
    }
}
