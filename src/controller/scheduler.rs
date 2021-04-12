use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Instant;

use crate::controller::types::*;
use crate::common::{Error, Token, RequestToken};

/// Data structure for adding and allocating hosts.
pub struct HostScheduler {
    /// Password required for a host to register with the controller.
    password: String,
    /// Index for the next host to allocate.
    prev_host_i: usize,
    /// Array of ordered hosts.
    hosts: Vec<HostID>,
    /// Enforces unique host names.
    unique_hosts: HashMap<HostID, Host>,
}

/// Data structure so the controller knows how to contact the host.
pub struct HostResult {
    /// Host IP.
    pub ip: String,
    /// Host port.
    pub port: u16,
    /// Request token to include in the compute request.
    pub request_token: RequestToken,
}

impl HostScheduler {
    /// Host metadata.
    #[cfg(test)]
    pub fn md(&self, id: &str) -> &HostMetadata {
        &self.unique_hosts.get(id).unwrap().md
    }

    pub fn new(password: &str) -> Self {
        Self {
            password: password.to_string(),
            prev_host_i: 0,
            hosts: Vec::new(),
            unique_hosts: HashMap::new(),
        }
    }

    /// List of hosts.
    pub fn hosts(&self) -> Vec<Host> {
        self.unique_hosts.values().map(|host| host.clone()).collect()
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
        id: &str,
        addr: SocketAddr,
        confirmed: bool,
        password: &str,
    ) -> bool {
        if password != self.password {
            warn!("incorrect password from {} ({:?})", &id, addr);
            false
        } else {
            if self.unique_hosts.contains_key(id) {
                return false;
            }
            info!("ADDED host {:?} {:?}", id, addr);
            self.unique_hosts.insert(
                id.to_string(),
                Host {
                    confirmed,
                    id: id.to_string(),
                    index: self.hosts.len(),
                    addr,
                    md: Default::default(),
                },
            );
            self.hosts.push(id.to_string());
            true
        }
    }

    pub fn remove_host(&mut self, id: &str) -> bool {
        let removed_i = if let Some(host) = self.unique_hosts.remove(id) {
            self.hosts.remove(host.index);
            host.index
        } else {
            return false;
        };
        info!("REMOVED host {:?}", id);
        for (_, host) in self.unique_hosts.iter_mut() {
            if host.index > removed_i {
                host.index -= 1;
            }
        }
        true
    }

    /// Find a host to connect to round-robin.
    ///
    /// A host is available if there is at least one host is registered,
    /// the host does not have an active request, and the last message from
    /// the host was received less than two heartbeat intervals ago.
    pub fn find_host(&mut self) -> Option<HostResult> {
        if self.hosts.is_empty() {
            return None;
        }
        let mut host_i = self.prev_host_i;
        for _ in 0..self.hosts.len() {
            host_i = (host_i + 1) % self.hosts.len();
            let host_id = &self.hosts[host_i];
            let host = self.unique_hosts.get_mut(host_id).unwrap();
            if host.md.active_request.is_some() || !host.is_confirmed() {
                continue;
            }
            let elapsed = host.md.last_msg.elapsed().as_secs();
            if elapsed > 2 * crate::host::HEARTBEAT_INTERVAL {
                continue;
            }
            if let Some(token) = host.md.token.take() {
                self.prev_host_i = host_i;
                println!("find_host picked => {:?}", host.addr);
                return Some(HostResult {
                    ip: host.addr.ip().to_string(),
                    port: host.addr.port(),
                    request_token: token,
                });
            }
        }
        None
    }

    /// Verify messages from a host actually came from the host.
    ///
    /// The `host_id` is the name of the host provided in messages
    /// of the following types: HostHeartbeat, NotifyStart, NotifyEnd.
    ///
    /// Returns: An error if a host with the name does not exist, or OK.
    #[allow(unused_variables)]
    pub fn verify_host_name(&self, host_id: &str) -> Result<(), Error> {
        let ip = self.unique_hosts.iter()
            .map(|(_, host)| host)
            .filter(|host| &host.id == host_id)
            .map(|host| host.addr.ip())
            .collect::<Vec<_>>();
        if ip.is_empty() {
            return Err(Error::InvalidHostMessage(format!(
                "failed to find host with id => {}", host_id)));
        }
        if ip.len() > 1 {
            return Err(Error::InvalidHostMessage(format!(
                "found multiple hosts with id => {}", host_id)));
        }
        Ok(())
    }

    /// Notify the scheduler that a service is starting a request.
    ///
    /// Finds the host with the given host ID and sets the active
    /// request to the given description. Logs an error message if the host
    /// cannot be found, or an already active request is overwritten.
    pub fn notify_start(&mut self, id: HostID, description: String) {
        info!("notify start name={:?} description={:?}", id, description);
        if let Some(host) = self.unique_hosts.get_mut(&id) {
            if let Some(req) = &host.md.active_request {
                error!("overriding active request: {:?}", req)
            }
            host.md.last_msg = Instant::now();
            host.md.active_request = Some(Request::new(description));
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
    ///
    /// Also updates the request token.
    pub fn notify_end(&mut self, id: HostID, token: RequestToken) {
        info!("notify end name={:?} token={:?}", id, token);
        if let Some(host) = self.unique_hosts.get_mut(&id) {
            if host.md.token.is_some() {
                error!("either the host sent a heartbeat during an \
                    active request or the host handled a request \
                    without us actually allocating the host. careful!")
            }
            host.md.token = Some(token);
            if let Some(mut req) = host.md.active_request.take() {
                req.end = Some(Instant::now());
                host.md.last_msg = Instant::now();
                host.md.last_request = Some(req);
                host.md.total += 1;
            } else {
                error!("no active request, null notify end");
            }
        } else {
            error!("missing host");
        }
    }

    /// Handle a host heartbeat, updating the request token for the
    /// host with the given service name.
    ///
    /// Parameters:
    /// - id - Host ID.
    /// - token - New request token, or empty string if not renewed.
    pub fn heartbeat(&mut self, id: HostID, token: &str) {
        debug!("heartbeat {} {:?}", id, token);
        if let Some(host) = self.unique_hosts.get_mut(&id) {
            host.md.last_msg = Instant::now();
            if !token.is_empty() {
                host.md.token = Some(Token(token.to_string()));
            }
        } else {
            error!("missing host");
        }
    }

    /// Confirms a host. Authenticated in the web dashboard.
    pub fn confirm_host(&mut self, id: &HostID) {
        if let Some(host) = self.unique_hosts.get_mut(id) {
            if host.is_confirmed() {
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

#[cfg(test)]
mod test {
    use super::*;
    use std::thread;
    use std::net::IpAddr;
    use std::time::Duration;

    const PASSWORD: &str = "password";

    /// Add a host named "host<i>" with socket addr "0.0.0.0:808<i>".
    fn add_host_test(s: &mut HostScheduler, i: usize) {
        let id = format!("host{}", i);
        let addr: SocketAddr = format!("0.0.0.0:808{}", i).parse().unwrap();
        assert!(s.add_host(&id, addr, true, PASSWORD));
    }

    #[test]
    fn test_add_host() {
        let mut s = HostScheduler::new(PASSWORD);
        assert!(s.hosts.is_empty());
        assert!(s.unique_hosts.is_empty());

        // Add a host
        let id = "host1";
        let addr: SocketAddr = "127.0.0.1:8081".parse().unwrap();
        assert!(s.add_host(id, addr, false, PASSWORD));

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
        assert!(host.md.active_request.is_none());
        assert!(host.md.last_request.is_none());
        assert!(host.md.token.is_none());
        assert_eq!(host.md.total, 0);
        assert!(!host.confirmed);
    }

    #[test]
    fn test_add_host_password() {
        let mut s = HostScheduler::new(PASSWORD);
        let addr: SocketAddr = "127.0.0.1:8081".parse().unwrap();
        assert!(!s.add_host("host1", addr.clone(), false, "???"));
        assert!(s.add_host("host1", addr.clone(), false, PASSWORD));
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
        assert!(s.add_host("host1", addr.clone(), true, PASSWORD));
        assert!(!s.add_host("host1", addr.clone(), true, PASSWORD));
        assert!(s.remove_host("host1"));
        assert!(!s.remove_host("host1"));
    }

    #[test]
    fn test_find_unconfirmed_host() {
        let mut s = HostScheduler::new(PASSWORD);

        // Add an unconfirmed host
        let id = "host1".to_string();
        assert!(s.add_host(&id, "0.0.0.0:8081".parse().unwrap(), false, PASSWORD));
        assert!(!s.unique_hosts.get(&id).unwrap().is_confirmed());
        s.heartbeat(id.clone(), "requesttoken");
        assert!(s.find_host().is_none());

        // Confirm the host, and we should be able to discover it
        s.confirm_host(&id);
        let host = s.find_host();
        assert!(host.is_some());
    }

    #[test]
    fn test_find_host_non_blocking() {
        let mut s = HostScheduler::new(PASSWORD);
        let request_token = "requesttoken";

        // Add three hosts
        add_host_test(&mut s, 1);
        add_host_test(&mut s, 2);
        add_host_test(&mut s, 3);
        assert_eq!(s.hosts.clone(), vec![
            "host1".to_string(),
            "host2".to_string(),
            "host3".to_string(),
        ]);
        s.heartbeat("host1".to_string(), request_token);
        s.heartbeat("host2".to_string(), request_token);
        s.heartbeat("host3".to_string(), request_token);

        // Set last_request of a host, say host 2.
        // find_host returns 2 3 1 round-robin.
        s.unique_hosts.get_mut("host2").unwrap().md.last_request = Some(Request::default());
        let host = s.find_host().unwrap();
        assert_eq!(host.port, 8082);
        let host = s.find_host().unwrap();
        assert_eq!(host.port, 8083);
        let host = s.find_host().unwrap();
        assert_eq!(host.port, 8081);
        s.heartbeat("host1".to_string(), request_token);
        s.heartbeat("host2".to_string(), request_token);
        s.heartbeat("host3".to_string(), request_token);

        // Make host 3 busy. (Reset request tokens)
        // find_host should return 2 1 2 round-robin.
        s.unique_hosts.get_mut("host3").unwrap().md.active_request = Some(Request::default());
        let host = s.find_host().unwrap();
        assert_eq!(host.port, 8082);
        s.heartbeat("host2".to_string(), request_token);
        let host = s.find_host().unwrap();
        assert_eq!(host.port, 8081);
        s.heartbeat("host1".to_string(), request_token);
        let host = s.find_host().unwrap();
        assert_eq!(host.port, 8082);
        s.heartbeat("host2".to_string(), request_token);

        // Make host 1 and 2 busy.
        // find_host should fail.
        s.unique_hosts.get_mut("host1").unwrap().md.active_request = Some(Request::default());
        s.unique_hosts.get_mut("host2").unwrap().md.active_request = Some(Request::default());
        assert!(s.find_host().is_none());
    }

    #[test]
    fn test_find_host_request_tokens() {
        let mut s = HostScheduler::new(PASSWORD);
        let request_token = "requesttoken";

        // Add a host.
        add_host_test(&mut s, 1);
        add_host_test(&mut s, 2);
        assert!(s.find_host().is_none(), "no request tokens");
        s.heartbeat("host1".to_string(), request_token);
        assert!(s.find_host().is_some(), "set token in heartbeat");
        assert!(s.find_host().is_none(), "reset token");
        s.heartbeat("host1".to_string(), request_token);
        s.heartbeat("host2".to_string(), request_token);
        assert!(s.find_host().is_some());
        assert!(s.find_host().is_some());
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
    fn host_heartbeat_sets_request_token() {
        let mut s = HostScheduler::new(PASSWORD);
        let name = "host1".to_string();
        let token1 = "abc";
        let token2 = "def";
        add_host_test(&mut s, 1);
        assert!(
            s.md(&name).token.is_none(),
            "no token initially",
        );
        s.heartbeat(name.clone(), "");
        assert!(
            s.md(&name).token.is_none(),
            "empty token doesn't change the token",
        );
        s.heartbeat(name.clone(), token1);
        assert_eq!(
            s.md(&name).token,
            Some(Token(token1.to_string())),
            "heartbeat sets the initial token",
        );
        s.heartbeat(name.clone(), "");
        assert_eq!(
            s.md(&name).token,
            Some(Token(token1.to_string())),
            "empty token doesn't change the token",
        );
        s.heartbeat(name.clone(), token2);
        assert_eq!(
            s.md(&name).token,
            Some(Token(token2.to_string())),
            "heartbeat can also replace tokens",
        );
    }

    #[test]
    fn host_messages_update_last_msg_time() {
        let mut s = HostScheduler::new(PASSWORD);
        let name = "host1".to_string();
        add_host_test(&mut s, 1);

        let t1 = s.md(&name).last_msg.clone();
        thread::sleep(Duration::from_secs(1));
        s.heartbeat(name.clone(), "token");
        let t2 = s.md(&name).last_msg.clone();
        thread::sleep(Duration::from_secs(1));
        s.heartbeat(name.clone(), "");
        let t3 = s.md(&name).last_msg.clone();
        thread::sleep(Duration::from_secs(1));
        s.notify_start(name.clone(), "description".to_string());
        let t4 = s.md(&name).last_msg.clone();
        thread::sleep(Duration::from_secs(1));
        s.notify_end(name.clone(), Token("token".to_string()));
        let t5 = s.md(&name).last_msg.clone();
        assert!(t2 > t1, "regular heartbeat updates time");
        assert!(t3 > t2, "empty heartbeat also updates time");
        assert!(t4 > t3, "notify start updates time");
        assert!(t5 > t4, "notify end updates time");
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
        let request_token1 = Token("requesttoken1".to_string());
        let request_token2 = Token("requesttoken2".to_string());
        add_host_test(&mut s, 1);

        // No requests initially.
        assert_eq!(s.md(&host1).total, 0);
        s.heartbeat(host1.clone(), &request_token1.0);
        assert_eq!(s.md(&host1).total, 0);
        let host = s.find_host();
        assert!(host.is_some());
        assert_eq!(host.unwrap().request_token, request_token1);
        assert_eq!(s.md(&host1).total, 0);
        s.notify_start(host1.clone(), "description".to_string());
        assert_eq!(s.md(&host1).total, 0);

        // One request.
        s.notify_end(host1.clone(), request_token2.clone());
        assert_eq!(s.md(&host1).total, 1);
        let host = s.find_host();
        assert!(host.is_some());
        assert_eq!(host.unwrap().request_token, request_token2);
        assert_eq!(s.md(&host1).total, 1);
        s.notify_start(host1.clone(), "description".to_string());
        assert_eq!(s.md(&host1).total, 1);

        // Two requests.
        s.notify_end(host1.clone(), request_token1.clone());
        assert_eq!(s.md(&host1).total, 2);
    }

    #[test]
    fn test_verify_host() {
        let mut s = HostScheduler::new(PASSWORD);
        let addr: SocketAddr = "1.2.3.4:8080".parse().unwrap();
        let ip: IpAddr = addr.ip();
        assert!(s.add_host("host1", addr, true, PASSWORD));

        assert!(s.verify_host_name("host2").is_err(), "invalid host name");
        assert!(s.verify_host_name("host1").is_ok(), "valid host name");
    }
}
