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

    /// Triggered data should only be read once.
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

#[cfg(test)]
mod test {
    use super::*;

    fn new_compute_request() -> ComputeRequest {
        ComputeRequest {
            host_token: "abcd1234".to_string(),
            module_id: "module".to_string(),
            cached: true,
            package: vec![2, 2, 2, 2],
            binary_path: "binary".to_string(),
            args: vec!["image.png".to_string()],
            envs: vec!["MODULE_ID=module".to_string()],
            params: "".to_string(),
            returns: "".to_string(),
            network_perm: vec![],
            triggered_tag: "".to_string(),
            triggered_timestamp: "".to_string(),
            triggered_data: vec![],
        }
    }

    #[test]
    fn basic_permissions_are_parsed() {
        let mut req = new_compute_request();
        let domain = "google.com".to_string();
        req.network_perm = vec![domain.clone()];

        let perms = ProcessPerms::new(&mut req);
        assert!(perms.start <= Instant::now());
        assert_eq!(perms.count, 0);
        assert!(perms.warm_cache_notify.is_none());
        assert_eq!(&perms.triggered_tag, "");
        assert_eq!(&perms.triggered_timestamp, "");
        assert!(perms.triggered_data.is_none());
        assert!(perms.read_perms.is_empty());
        assert!(perms.write_perms.is_empty());
        assert_eq!(perms.network_perms.len(), 1);
        assert!(perms.network_perms.contains(&domain));
    }

    #[test]
    fn triggered_data_is_parsed_in_normal_case() {
        let mut req = new_compute_request();
        let tag = "t1".to_string();
        let timestamp = "10:00".to_string();
        let data = vec![5, 5, 5, 5];
        req.triggered_tag = tag.clone();
        req.triggered_timestamp = timestamp.clone();
        req.triggered_data = data.clone();

        let perms = ProcessPerms::new(&mut req);
        assert!(!req.triggered_tag.is_empty());
        assert!(!req.triggered_timestamp.is_empty());
        assert!(req.triggered_data.is_empty(), "data is not copied");
        assert_eq!(perms.triggered_tag, tag);
        assert_eq!(perms.triggered_timestamp, timestamp);
        assert_eq!(perms.triggered_data, Some(data));
    }

    #[test]
    fn triggered_data_is_parsed_even_when_empty() {
        let mut req = new_compute_request();
        let tag = "t1".to_string();
        let timestamp = "10:00".to_string();
        req.triggered_tag = tag.clone();
        req.triggered_timestamp = timestamp.clone();
        req.triggered_data = vec![];

        let perms = ProcessPerms::new(&mut req);
        assert_eq!(perms.triggered_tag, tag);
        assert_eq!(perms.triggered_timestamp, timestamp);
        assert_eq!(perms.triggered_data, Some(vec![]));
    }

    #[test]
    fn read_write_perms_are_parsed() {
        let mut req = new_compute_request();
        let params = "a;t1:b;t2:c;t3".to_string();
        let returns = "x;t4:y;t5,t6,t7".to_string();
        req.params = params.clone();
        req.returns = returns.clone();

        let perms = ProcessPerms::new(&mut req);
        assert_eq!(perms.read_perms.len(), 3);
        assert!(perms.read_perms.contains("t1"));
        assert!(perms.read_perms.contains("t2"));
        assert!(perms.read_perms.contains("t3"));
        assert_eq!(perms.write_perms.len(), 4);
        assert!(perms.write_perms.contains("t4"));
        assert!(perms.write_perms.contains("t5"));
        assert!(perms.write_perms.contains("t6"));
        assert!(perms.write_perms.contains("t7"));
    }

    #[test]
    fn is_and_read_triggered() {
        let mut req = new_compute_request();
        let tag = "t1".to_string();
        let timestamp = "10:00".to_string();
        let data = vec![5, 5, 5, 5];
        req.triggered_tag = tag.clone();
        req.triggered_timestamp = timestamp.clone();
        req.triggered_data = data.clone();

        let mut perms = ProcessPerms::new(&mut req);
        assert!(perms.triggered_data.is_some());
        assert!(!perms.is_triggered("t2"));
        assert!(perms.is_triggered("t1"));
        assert!(perms.read_triggered("9:00").is_none(), "incorrect timestamp");
        assert!(perms.read_triggered(&timestamp).is_some());
        assert!(perms.triggered_data.is_none(), "can't read triggered data twice");
        assert!(perms.read_triggered(&timestamp).is_none(), "can't read triggered data twice");
    }

    #[test]
    fn use_triggered_key() {
        let mut req = new_compute_request();
        let tag = "t1".to_string();
        let timestamp = "10:00".to_string();
        let data = vec![5, 5, 5, 5];
        req.triggered_tag = tag.clone();
        req.triggered_timestamp = timestamp.clone();
        req.triggered_data = data.clone();

        let mut perms = ProcessPerms::new(&mut req);
        assert!(&tag != TRIGGERED_KEY);
        assert!(&timestamp != TRIGGERED_KEY);
        assert!(perms.triggered_data.is_some());
        assert!(perms.is_triggered(TRIGGERED_KEY));
        assert!(perms.read_triggered(TRIGGERED_KEY).is_some());
        assert!(perms.triggered_data.is_none(), "can't read triggered data twice");
        assert!(perms.read_triggered(&timestamp).is_none(), "can't read triggered data twice");
    }

    #[test]
    fn test_read_write_network_permissions() {
        let mut req = new_compute_request();
        let params = "a;t1:b;t2:c;t3".to_string();
        let returns = "x;t4:y;t5,t6,t7".to_string();
        let domain = "google.com".to_string();
        req.params = params.clone();
        req.returns = returns.clone();
        req.network_perm = vec![domain.clone()];

        let     perms = ProcessPerms::new(&mut req);
        assert!(perms.can_read("t1"));
        assert!(perms.can_write("t4"));
        assert!(perms.can_access_domain(&domain));
        assert!(!perms.can_read("t4"));
        assert!(!perms.can_write("t1"));
        assert!(!perms.can_access_domain("yahoo.com"));
        assert!(!perms.can_read(TRIGGERED_KEY));
        assert!(!perms.can_write(TRIGGERED_KEY));
    }

    #[tokio::test]
    async fn test_warm_cache_functionality() {
        let mut req = new_compute_request();
        let mut perms = ProcessPerms::new(&mut req);
        assert!(perms.touch().is_none());

        let (mut perms, tx) = ProcessPerms::new_warm_cache();
        let rx = perms.touch();
        assert!(rx.is_some());
        assert!(tx.send(()).await.is_ok(), "can send message");
        assert!(rx.unwrap().recv().await.is_some(), "message is received");
    }
}

