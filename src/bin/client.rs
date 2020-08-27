#[macro_use]
extern crate log;

use std::fs::File;
use std::io::Read;
use std::time::Duration;
use std::thread;

use tokio::runtime::Runtime;
use karl::{controller::{Controller, HostConnection}, ComputeRequest};

fn main() {
    env_logger::builder().format_timestamp(None).init();
    let rt = Runtime::new().unwrap();

    let mut c = Controller::new(rt, Duration::from_secs(10));
    let hosts = loop {
        let hosts = c.find_hosts();
        if hosts.is_empty() {
            debug!("No hosts found! Try again in 1 second...");
            thread::sleep(Duration::from_secs(1));
        } else {
            break hosts;
        }
    };

    let host = hosts[0];  // Take the first host.
    info!("connecting to {:?}", host);
    let mut conn = HostConnection::connect(host).unwrap();
    info!("pinging {:?}", conn.host_addr());
    match conn.ping() {
        Ok(Some(_)) => {},
        Ok(None) => {
            warn!("could not be reached! (ping)");
            return;
        },
        Err(e) => {
            error!("error pinging host: {:?}", e);
            return;
        }
    }

    info!("connecting to {:?}", host);
    let mut conn = HostConnection::connect(host).unwrap();
    info!("reading package.zip");
    let mut f = File::open("package.zip").expect("failed to open package.zip");
    let mut buffer: Vec<u8> = Vec::new();
    f.read(&mut buffer).expect("failed to read package.zip");
    info!("sending compute request");
    match conn.execute(ComputeRequest::new(buffer)) {
        Ok(Some(res)) => info!("Result: {:?}", res),
        Ok(None) => warn!("could not be reached! (compute)"),
        Err(e) => error!("error contacting host: {:?}", e),
    }
}
