use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mdns;
use tokio;
use futures_util::{pin_mut, stream::StreamExt};
use tokio::runtime::Runtime;

use crate::*;

/// The hostname of the devices we are searching for.
const SERVICE_NAME: &'static str = "karl._tcp._tcp.local";

/// Controller for interacting with mDNS.
pub struct Controller {
    rt: Runtime,
    hosts: Arc<Mutex<HashSet<SocketAddr>>>,
}

/// Connection to a host.
pub struct HostConnection {
    host: SocketAddr,
}

impl Controller {
    /// Create a controller that asynchronously queries for new hosts
    /// at the given interval.
    pub fn new(rt: Runtime, interval: Duration) -> Self {
        let c = Controller { rt, hosts: Arc::new(Mutex::new(HashSet::new())) };
        let hosts = c.hosts.clone();
        c.rt.spawn(async move {
            let stream = mdns::discover::all(SERVICE_NAME, interval)
                .expect("TODO")
                .listen();
            pin_mut!(stream);
            while let Some(Ok(response)) = stream.next().await {
                if response.is_empty() {
                    continue;
                }
                if let Some(addr) = response.socket_address() {
                    println!("discovered host: {:?}", addr);
                    hosts.lock().unwrap().insert(addr);
                }
            }
        });
        c
    }

    /// Find all hosts that broadcast the service over mDNS.
    pub fn find_hosts(&mut self) -> Vec<SocketAddr> {
        let mut hosts = self.hosts
            .lock()
            .unwrap()
            .clone()
            .into_iter()
            .collect::<Vec<_>>();
        hosts.sort();
        hosts
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
