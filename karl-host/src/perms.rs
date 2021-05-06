use std::collections::HashSet;
use std::time::Instant;
use tokio::sync::mpsc;

use crate::protos::*;

/// Special key to get the triggered data regardless of tag/timestamp
pub const TRIGGERED_KEY: &str = "triggered";

/// Permissions of an active process
#[derive(Debug)]
pub struct ProcessPerms {
    start: Instant,
    count: usize,
    warm_cache_notify: Option<mpsc::Receiver<()>>,
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

impl Default for ProcessPerms {
    fn default() -> Self {
        Self {
            start: Instant::now(),
            count: 0,
            warm_cache_notify: None,
            triggered_tag: String::new(),
            triggered_timestamp: String::new(),
            triggered_data: None,
            read_perms: HashSet::new(),
            write_perms: HashSet::new(),
            network_perms: HashSet::new(),
        }
    }
}

impl ProcessPerms {
    pub fn new(req: &mut ComputeRequest) -> Self {
        let mut perms = ProcessPerms::default();
        perms.set_compute_request(req);
        perms
    }

    pub fn set_compute_request(&mut self, req: &mut ComputeRequest) {
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
        self.start = Instant::now();
        self.triggered_tag = req.triggered_tag.clone();
        self.triggered_timestamp = req.triggered_timestamp.clone();
        self.triggered_data = triggered_data;
        self.read_perms = read_perms;
        self.write_perms = write_perms;
        self.network_perms = network_perms;
    }

    pub fn new_warm_cache() -> (Self, mpsc::Sender<()>) {
        let buffer_size = 1;
        let (tx, rx) = mpsc::channel::<()>(buffer_size);
        let mut perms = ProcessPerms::default();
        perms.warm_cache_notify = Some(rx);
        (perms, tx)
    }

    pub fn is_triggered(&self, tag: &str) -> bool {
        tag == TRIGGERED_KEY || tag == self.triggered_tag
    }

    pub fn read_triggered(&mut self, timestamp: &str) -> Option<Vec<u8>> {
        if timestamp == self.triggered_timestamp {
            self.triggered_data.take()
        } else if timestamp == TRIGGERED_KEY {
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

    // Returns a receiving channel to wait on if it's a warm cache.
    pub fn touch(&mut self) -> Option<mpsc::Receiver<()>> {
        if self.count == 0 {
            debug!("initializing took {:?} until first perm used", Instant::now() - self.start);
        }
        self.count += 1;
        self.warm_cache_notify.take()
    }
}
