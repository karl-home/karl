use std::fs::File;
use std::io::Read;
use std::time::Duration;
use std::thread;

use tokio::runtime::Runtime;
use karl::{controller::{Controller, HostConnection}, ComputeRequest};

fn main() {
    let rt = Runtime::new().unwrap();

    let mut c = Controller::new(rt, Duration::from_secs(10));
    let hosts = loop {
        let hosts = c.find_hosts();
        if hosts.is_empty() {
            println!("No hosts found! Try again in 1 second...");
            thread::sleep(Duration::from_secs(1));
        } else {
            break hosts;
        }
    };

    let host = hosts[0];  // Take the first host.
    let mut conn = HostConnection::connect(host).unwrap();
    match conn.ping() {
        Ok(Some(_)) => {},
        Ok(None) => {
            println!("Host {:?} could not be reached! (ping)", conn.host_addr());
            return;
        },
        Err(e) => {
            println!("Error pinging host {:?}: {:?}", conn.host_addr(), e);
            return;
        }
    }

    let mut f = File::open("package.zip").expect("failed to open package.zip");
    let mut buffer: Vec<u8> = Vec::new();
    f.read(&mut buffer).expect("failed to read package.zip");
    match conn.execute(ComputeRequest::new(buffer)) {
        Ok(Some(res)) => println!("Result: {:?}", res),
        Ok(None) => println!("Host {:?} could not be reached! (compute)", conn.host_addr()),
        Err(e) => println!("Error contacting host {:?}: {:?}", conn.host_addr(), e),
    }
}
