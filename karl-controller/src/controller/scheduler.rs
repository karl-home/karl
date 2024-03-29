use std::collections::{HashSet, HashMap};
use std::net::SocketAddr;
use std::time::Instant;

use tonic::{Status, Code};
use karl_common::*;

const REQUEST_THRESHOLD: usize = 10;

/// Host status and information.
#[derive(Debug, Clone)]
pub struct Host {
    /// Whether the user has confirmed this host.
    pub confirmed: bool,
    /// Host ID.
    pub id: HostID,
    /// Host address.
    pub addr: SocketAddr,
    /// Metadata.
    pub md: HostMetadata,
}

#[derive(Debug, Clone)]
pub struct HostMetadata {
    /// Cached module IDs:
    pub cached_modules: HashSet<ModuleID>,
    /// All active requests.
    pub active_requests: HashSet<ProcessToken>,
    /// Time of last heartbeat, notify start, or notify end.
    pub last_msg: Instant,
    /// Total number of requests handled.
    pub total: usize,
}

/// Data structure for adding and allocating hosts.
pub struct HostScheduler {
    /// Password required for a host to register with the controller.
    password: String,
    /// Enforces unique host names.
    hosts: HashMap<HostToken, Host>,
    /// Whether caching is enabled.
    caching_enabled: bool,
}

/// Data structure so the controller knows how to contact the host.
pub struct HostResult {
    /// Host token.
    pub host_token: HostToken,
    /// Host IP.
    pub ip: String,
    /// Host port.
    pub port: u16,
    /// Whether the include module ID is cached.
    pub cached: bool,
}

impl Default for HostMetadata {
    fn default() -> Self {
        Self {
            cached_modules: HashSet::new(),
            active_requests: HashSet::new(),
            last_msg: Instant::now(),
            total: 0,
        }
    }
}

impl HostScheduler {
    /// Host metadata.
    #[cfg(test)]
    pub fn md(&self, id: &str) -> &HostMetadata {
        &self.hosts.get(id).unwrap().md
    }

    pub fn new(password: &str, caching_enabled: bool) -> Self {
        Self {
            password: password.to_string(),
            hosts: HashMap::new(),
            caching_enabled,
        }
    }

    /// List of hosts.
    pub fn hosts(&self) -> Vec<Host> {
        self.hosts.values().map(|host| host.clone()).collect()
    }

    /// Add a host.
    ///
    /// Parameters:
    /// - id - The host ID.
    /// - addr - The address of the host.
    /// - confirmed - Whether the host should be confirmed by default.
    /// - password - Controller password known by the host.
    ///
    /// Returns:
    /// Whether the host was added. Not added if it is a duplicate by ID,
    /// or if the password was incorrect.
    pub fn add_host(
        &mut self,
        id: HostID,
        addr: SocketAddr,
        confirmed: bool,
        password: &str,
    ) -> Option<HostToken> {
        if password != self.password {
            warn!("incorrect password from {} ({:?})", &id, addr);
            None
        } else {
            for host in self.hosts.values() {
                if id == host.id {
                    return None;
                }
            }
            let host_token: HostToken = Token::gen();
            info!("ADDED host {} {:?} {}", id, addr, host_token);
            assert!(!self.hosts.contains_key(&host_token));
            self.hosts.insert(
                host_token.clone(),
                Host {
                    confirmed,
                    id,
                    addr,
                    md: Default::default(),
                },
            );
            Some(host_token)
        }
    }

    pub fn remove_host(&mut self, id: &str) -> bool {
        let hosts = self.hosts.iter()
            .filter(|(_, host)| &host.id == id)
            .map(|(token, _)| token.clone())
            .collect::<Vec<_>>();
        if !hosts.is_empty() {
            for token in hosts {
                info!("removed host {:?}", self.hosts.remove(&token));
            }
            true
        } else {
            warn!("cannot remove host with id {}: does not exist", id);
            false
        }
    }

    /// Find a host to connect.
    ///
    /// A host is available if there is at least one host is registered,
    /// and the last message from  the host was received less than two
    /// heartbeat intervals ago.
    ///
    /// Picks host with the least number of active requests. Prioritizes
    /// those with the cached module ID, unless has more than REQUEST_THRESHOLD
    /// requests. Set threshold based on average execution time of module ID,
    /// but otherwise 10.
    pub fn find_hosts(&mut self, module_id: &ModuleID) -> Vec<HostResult> {
        let mut hosts = self.hosts.iter()
            .filter(|(_, host)| host.confirmed)
            .filter(|(_, host)| {
                let elapsed = host.md.last_msg.elapsed().as_secs();
                elapsed <= 2 * HEARTBEAT_INTERVAL
            })
            .map(|(host_token, host)| {
                let cached = host.md.cached_modules.contains(module_id);
                (host_token, host, cached)
            })
            .collect::<Vec<_>>();
        hosts.sort_by_key(|(_, host, cached)| {
            let mut indicator = 1;
            if *cached && host.md.active_requests.len() < REQUEST_THRESHOLD {
                indicator = 0;
            }
            (indicator, host.md.active_requests.len())
        });
        hosts.iter().map(|(host_token, host, cached)| {
            HostResult {
                host_token: host_token.to_string(),
                ip: host.addr.ip().to_string(),
                port: host.addr.port(),
                cached: *cached,
            }
        }).collect()
    }

    /// Notify the scheduler that a service is starting a request.
    ///
    /// Finds the host with the given host ID and sets the active
    /// request to the given description. Logs an error message if the host
    /// cannot be found, or an already active request is overwritten.
    pub fn notify_start(
        &mut self,
        host_token: HostToken,
        module_id: ModuleID,
        process_token: ProcessToken,
    ) {
        if let Some(host) = self.hosts.get_mut(&host_token) {
            host.md.last_msg = Instant::now();
            host.md.active_requests.insert(process_token);
            if self.caching_enabled {
                host.md.cached_modules.insert(module_id);
            }
            trace!("notify start host_id={} total={}", host.id, host.md.active_requests.len());
        } else {
            error!("missing host");
        }
    }

    /// Notify the scheduler that a service is ending a request.
    ///
    /// Finds the host with the given host ID and sets the last request
    /// to be the previously active request, updating the end time. Logs an
    /// error message if the host cannot be found, or if the host does not
    /// have an active request.
    pub fn notify_end(
        &mut self, host_token: HostToken, process_token: ProcessToken
    ) -> Result<(), Status> {
        if let Some(host) = self.hosts.get_mut(&host_token) {
            host.md.last_msg = Instant::now();
            host.md.active_requests.remove(&process_token);
            host.md.total += 1;
            trace!("notify end host_id={} total={}", host.id, host.md.active_requests.len());
            Ok(())
        } else {
            error!("missing host");
            Err(Status::new(Code::Unauthenticated, "invalid host token"))
        }
    }

    /// Handle a host heartbeat. Validates the host token belongs to a host,
    /// then updates the last contacted time for the host.
    ///
    /// Parameters:
    /// - token - The host token identifying the host.
    pub fn heartbeat(&mut self, token: HostToken) {
        trace!("heartbeat {}", token);
        if let Some(host) = self.hosts.get_mut(&token) {
            host.md.last_msg = Instant::now();
        } else {
            error!("missing host");
        }
    }

    /// Confirms a host. Authenticated in the web dashboard.
    pub fn confirm_host(&mut self, id: &HostID) {
        let mut hosts = self.hosts.iter_mut()
            .map(|(_, host)| host)
            .filter(|host| &host.id == id);
        if let Some(host) = hosts.next() {
            if host.confirmed {
                warn!("attempted to confirm already confirmed host: {:?}", id);
            } else {
                info!("confirmed host {:?}", id);
                host.confirmed = true;
            }
        } else {
            warn!("attempted to confirm nonexistent host: {:?}", id);
        }
    }
}

/*
#[cfg(test)]
mod test {
    use super::*;
    use std::thread;
    use std::time::Duration;

    const PASSWORD: &str = "password";

    /// Add a host named "host<i>" with socket addr "0.0.0.0:808<i>".
    fn add_host_test(s: &mut HostScheduler, i: usize) -> HostToken {
        let id = format!("host{}", i);
        let addr: SocketAddr = format!("0.0.0.0:808{}", i).parse().unwrap();
        let host_token = s.add_host(id, addr, true, PASSWORD);
        assert!(host_token.is_some());
        host_token.unwrap()
    }

    #[test]
    fn test_add_host() {
        let mut s = HostScheduler::new(PASSWORD);
        assert!(s.hosts.is_empty());
        assert!(s.unique_hosts.is_empty());

        // Add a host
        let id = "host1";
        let addr: SocketAddr = "127.0.0.1:8081".parse().unwrap();
        assert!(s.add_host(id.to_string(), addr, false, PASSWORD).is_some());

        // Check scheduler was modified correctly
        assert_eq!(s.hosts.len(), 1);
        assert_eq!(s.unique_hosts.len(), 1);
        assert_eq!(s.hosts[0], id, "wrong host id");
        assert!(s.unique_hosts.get(id).is_some(), "violated host id invariant");

        // Check host was initialized correctly
        let host = s.unique_hosts.get(id).unwrap();
        assert_eq!(host.index, 0);
        assert_eq!(host.id, id);
        assert_eq!(host.addr, addr);
        assert_eq!(host.md.total, 0);
        assert!(!host.confirmed);
    }

    #[test]
    fn test_add_host_password() {
        let mut s = HostScheduler::new(PASSWORD);
        let addr: SocketAddr = "127.0.0.1:8081".parse().unwrap();
        assert!(s.add_host("host1".to_string(), addr.clone(), false, "???").is_some());
        assert!(s.add_host("host1".to_string(), addr.clone(), false, PASSWORD).is_some());
    }

    #[test]
    fn test_add_multiple_hosts() {
        let mut s = HostScheduler::new(PASSWORD);

        // Add three hosts
        add_host_test(&mut s, 1);
        add_host_test(&mut s, 2);
        add_host_test(&mut s, 3);

        // Check the index in unique_hosts corresponds to the index in hosts
        for (id, host) in s.unique_hosts.iter() {
            assert_eq!(id, &host.id);
            assert_eq!(s.hosts[host.index], host.id);
        }
    }

    #[test]
    fn test_remove_host() {
        let mut s = HostScheduler::new(PASSWORD);

        // Add hosts
        add_host_test(&mut s, 1);
        add_host_test(&mut s, 2);
        add_host_test(&mut s, 3);
        add_host_test(&mut s, 4);

        // Remove the last host
        assert!(s.hosts.contains(&"host4".to_string()));
        assert!(s.remove_host("host4"));
        assert!(!s.hosts.contains(&"host4".to_string()));
        assert_eq!(s.hosts.len(), 3);
        assert_eq!(s.unique_hosts.len(), 3);
        for host in s.unique_hosts.values() {
            assert_eq!(s.hosts[host.index], host.id);
        }

        // Remove the middle host
        assert!(s.hosts.contains(&"host2".to_string()));
        assert!(s.remove_host("host2"));
        assert!(!s.hosts.contains(&"host2".to_string()));
        assert_eq!(s.hosts.len(), 2);
        assert_eq!(s.unique_hosts.len(), 2);
        for host in s.unique_hosts.values() {
            assert_eq!(s.hosts[host.index], host.id);
        }

        // Remove the first host
        assert!(s.hosts.contains(&"host1".to_string()));
        assert!(s.remove_host("host1"));
        assert!(!s.hosts.contains(&"host1".to_string()));
        assert_eq!(s.hosts.len(), 1);
        assert_eq!(s.unique_hosts.len(), 1);
        for host in s.unique_hosts.values() {
            assert_eq!(s.hosts[host.index], host.id);
        }
    }

    #[test]
    fn test_add_remove_host_return_value() {
        let mut s = HostScheduler::new(PASSWORD);

        let addr: SocketAddr = "0.0.0.0:8081".parse().unwrap();
        assert!(s.add_host("host1".to_string(), addr.clone(), true, PASSWORD).is_some());
        assert!(s.add_host("host1".to_string(), addr.clone(), true, PASSWORD).is_none());
        assert!(s.remove_host("host1"));
        assert!(!s.remove_host("host1"));
    }

    #[test]
    fn test_find_unconfirmed_host() {
        let mut s = HostScheduler::new(PASSWORD);

        // Add an unconfirmed host
        let id = "host1".to_string();
        let t1 = s.add_host(id, "0.0.0.0:8081".parse().unwrap(), false, PASSWORD);
        assert!(t1.is_some());
        let t1 = t1.unwrap();
        assert!(!s.unique_hosts.get(&t1).unwrap().is_confirmed());
        s.heartbeat(t1.clone());
        assert!(s.find_host().is_none());

        // Confirm the host, and we should be able to discover it
        s.confirm_host(&t1);
        let host = s.find_host();
        assert!(host.is_some());
    }

    #[test]
    fn test_find_host_non_blocking() {
        let mut s = HostScheduler::new(PASSWORD);

        // Add three hosts
        let t1 = add_host_test(&mut s, 1);
        let t2 = add_host_test(&mut s, 2);
        let t3 = add_host_test(&mut s, 3);
        assert_eq!(s.hosts.clone(), vec![
            "host1".to_string(),
            "host2".to_string(),
            "host3".to_string(),
        ]);
        s.heartbeat(t1.clone());
        s.heartbeat(t2.clone());
        s.heartbeat(t3.clone());

        // Set last_request of a host, say host 2.
        // find_host returns 2 3 1 round-robin.
        s.unique_hosts.get_mut(&t2).unwrap().md.last_request = Some(Request::default());
        let host = s.find_host().unwrap();
        assert_eq!(host.port, 8082);
        let host = s.find_host().unwrap();
        assert_eq!(host.port, 8083);
        let host = s.find_host().unwrap();
        assert_eq!(host.port, 8081);
        s.heartbeat(t1.clone());
        s.heartbeat(t2.clone());
        s.heartbeat(t3.clone());

        // Make host 3 busy. (Reset request tokens)
        // find_host should return 2 1 2 round-robin.
        s.unique_hosts.get_mut("host3").unwrap().md.active_request = Some(Request::default());
        let host = s.find_host().unwrap();
        assert_eq!(host.port, 8082);
        s.heartbeat(t2.clone());
        let host = s.find_host().unwrap();
        assert_eq!(host.port, 8081);
        s.heartbeat(t1.clone());
        let host = s.find_host().unwrap();
        assert_eq!(host.port, 8082);
        s.heartbeat(t2.clone());

        // Make host 1 and 2 busy.
        // find_host should fail.
        s.unique_hosts.get_mut(&t1).unwrap().md.active_request = Some(Request::default());
        s.unique_hosts.get_mut(&t2).unwrap().md.active_request = Some(Request::default());
        assert!(s.find_host().is_none());
    }

    #[test]
    fn test_notify_start_no_hosts() {
        let mut s = HostScheduler::new(PASSWORD);

        // Notify start with no hosts. Nothing errors.
        let host_id = "host1".to_string();
        let description = "my first app :)";
        s.notify_start(host_id.clone(), description.to_string());

        // Create a host and notify start.
        add_host_test(&mut s, 1);
        assert!(s.md("host1").active_request.is_none(),
            "no initial active request");
        s.notify_start(host_id.clone(), description.to_string());
        let request = s.md("host1").active_request.clone();
        assert!(request.is_some(), "active request started");
        let request = request.unwrap();
        assert_eq!(request.description, description, "same description");
        assert!(request.end.is_none(), "request does not have an end time");

        // Notify start again and overwrite the old request.
        thread::sleep(Duration::from_secs(2));
        s.notify_start(host_id.clone(), "what??".to_string());
        let new_request = s.md("host1").active_request.clone();
        assert!(new_request.is_some());
        let new_request = new_request.unwrap();
        assert!(new_request.description != description, "description changed");
        assert!(new_request.start > request.start, "start time increased");
        assert!(new_request.end.is_none(), "end time still does not exist");
    }

    #[test]
    fn test_notify_end() {
        let mut s = HostScheduler::new(PASSWORD);

        // Notify end with no hosts. Nothing errors.
        let host_id = "host1".to_string();
        let description = "description".to_string();
        let token = Token("abc123".to_string());
        s.notify_end(host_id.clone(), token.clone());

        // Create a host. Notify end does not do anything without an active request.
        add_host_test(&mut s, 1);
        assert!(s.md("host1").active_request.is_none());
        assert!(s.md("host1").last_request.is_none());
        s.notify_end(host_id.clone(), token.clone());
        assert!(s.md("host1").active_request.is_none());
        assert!(s.md("host1").last_request.is_none());

        // Notify start then notify end.
        s.notify_start(host_id.clone(), description.clone());
        thread::sleep(Duration::from_secs(2));
        s.notify_end(host_id.clone(), token.clone());
        assert!(s.md("host1").active_request.is_none());
        assert!(s.md("host1").last_request.is_some());
        let request = s.md("host1").last_request.clone().unwrap();
        assert!(request.description == description);
        assert!(request.end.is_some());
        assert!(request.end.unwrap() > request.start);
    }

    #[test]
    fn host_messages_update_last_msg_time() {
        let mut s = HostScheduler::new(PASSWORD);
        let name = "host1".to_string();
        let host_token = add_host_test(&mut s, 1);

        let t1 = s.md(&name).last_msg.clone();
        thread::sleep(Duration::from_secs(1));
        s.heartbeat(host_token.clone());
        let t2 = s.md(&name).last_msg.clone();
        thread::sleep(Duration::from_secs(1));
        s.heartbeat(host_token);
        let t3 = s.md(&name).last_msg.clone();
        // thread::sleep(Duration::from_secs(1));
        // s.notify_start(name.clone(), "description".to_string());
        // let t4 = s.md(&name).last_msg.clone();
        // thread::sleep(Duration::from_secs(1));
        // s.notify_end(name.clone(), "token".to_string());
        // let t5 = s.md(&name).last_msg.clone();
        assert!(t2 > t1, "regular heartbeat updates time");
        assert!(t3 > t2, "empty heartbeat also updates time");
        // assert!(t4 > t3, "notify start updates time");
        // assert!(t5 > t4, "notify end updates time");
    }

    #[test]
    fn test_notify_end_also_resets_request_tokens() {
        let mut s = HostScheduler::new(PASSWORD);

        let host1 = "host1".to_string();
        let request_token1 = Token("requesttoken1".to_string());
        let request_token2 = Token("requesttoken2".to_string());
        add_host_test(&mut s, 1);

        // Heartbeat
        assert!(s.md(&host1).token.is_none());
        s.heartbeat(host1.clone(), &request_token1.0);
        assert!(s.md(&host1).token.is_some());

        // HostRequest
        let host = s.find_host();
        assert!(host.is_some());
        assert!(s.md(&host1).token.is_none());
        assert_eq!(host.unwrap().request_token, request_token1);

        // NotifyStart
        s.notify_start(host1.clone(), "description".to_string());
        assert!(s.md(&host1).token.is_none());

        // NotifyEnd
        s.notify_end(host1.clone(), request_token2.clone());
        assert!(s.md(&host1).token.is_some());

        // HostRequest
        let host = s.find_host();
        assert!(host.is_some());
        assert!(s.md(&host1).token.is_none());
        assert_eq!(host.unwrap().request_token, request_token2);
    }

    #[test]
    fn test_notify_end_updates_total_number_of_requests() {
        let mut s = HostScheduler::new(PASSWORD);

        let host1 = "host1".to_string();
        let host_token = add_host_test(&mut s, 1);

        // No requests initially.
        assert_eq!(s.md(&host1).total, 0);
        s.heartbeat(host1.clone(), &request_token1.0);
        assert_eq!(s.md(&host1).total, 0);
        let host = s.find_host();
        assert!(host.is_some());
        assert_eq!(s.md(&host1).total, 0);
        s.notify_start(host1.clone(), "description".to_string());
        assert_eq!(s.md(&host1).total, 0);

        // One request.
        s.notify_end(host1.clone());
        assert_eq!(s.md(&host1).total, 1);
        let host = s.find_host();
        assert!(host.is_some());
        assert_eq!(s.md(&host1).total, 1);
        s.notify_start(host1.clone());
        assert_eq!(s.md(&host1).total, 1);

        // Two requests.
        s.notify_end(host1.clone(), request_token1.clone());
        assert_eq!(s.md(&host1).total, 2);
    }
}
*/