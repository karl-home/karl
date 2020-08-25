use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::runtime::Runtime;

use crate::*;

/// Controller for interacting with mDNS.
pub struct Controller {
    hosts: Arc<Mutex<Vec<SocketAddr>>>,
}

/// Connection to a host.
pub struct HostConnection {
    host: SocketAddr,
}

impl Controller {
    /// Create a controller that asynchronously queries for new hosts
    /// at the given interval.
    pub fn new(rt: Runtime, interval: Duration) -> Self {
        unimplemented!()
    }

    /// Find all hosts that broadcast the service over mDNS.
    pub fn find_hosts(&mut self) -> Vec<SocketAddr> {
        unimplemented!();
    }
}

impl HostConnection {
    /// Connect to a host.
    pub fn connect(host: SocketAddr) -> Self {
        unimplemented!();
    }

    /// Returns the address of the connected host.
    pub fn host_addr(&self) -> &SocketAddr {
        &self.host
    }

    /// Send a request to the connected host.
    fn send(&self, req: KarlRequest) -> Option<KarlResult> {
        unimplemented!();
    }

    /// Ping the host.
    pub fn ping(&self) -> Option<PingResult> {
        unimplemented!();
    }

    /// Execute a compute request.
    pub fn execute(&self, req: ComputeRequest) -> Option<ComputeResult> {
        unimplemented!();
    }
}
