use std::collections::HashSet;
use std::time::Instant;

use crate::protos::*;

/// Permissions of an active process
#[derive(Debug)]
pub struct ProcessPerms {
    start: Instant,
    count: usize,
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
    pub fn new(req: &mut ComputeRequest) -> Self {
        let triggered_data = if req.triggered_tag == "" || req.triggered_timestamp == "" {
            None
        } else {
            Some(req.triggered_data.drain(..).collect())
        };
        let read_perms = req.params
            .split(":")
            .map(|param| param.split(";"))
            .filter_map(|mut param| {
                if let Some(_) = param.next() {
                    param.next()
                } else {
                    None
                }
            })
            .map(|tag| tag.to_string())
            .filter(|tag| tag != &req.triggered_tag)
            .collect::<HashSet<String>>();
        let write_perms = req.returns
            .split(":")
            .map(|param| param.split(";"))
            .filter_map(|mut param| {
                if let Some(_) = param.next() {
                    param.next()
                } else {
                    None
                }
            })
            .flat_map(|tags| tags.split(",").map(|tag| tag.to_string()))
            .collect::<HashSet<String>>();
        let network_perms: HashSet<_> = req.network_perm.clone().into_iter().collect();
        Self {
            start: Instant::now(),
            count: 0,
            triggered_tag: req.triggered_tag.clone(),
            triggered_timestamp: req.triggered_timestamp.clone(),
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

    pub fn can_write(&self, tag: &str) -> bool {
        self.write_perms.contains(tag)
    }

    pub fn touch(&mut self) {
        if self.count == 0 {
            debug!("initializing process took {:?}", Instant::now() - self.start);
        }
        self.count += 1;
    }
}
