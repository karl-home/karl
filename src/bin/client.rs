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
    let conn = HostConnection::connect(host);
    if conn.ping().is_none() {
        println!("Host {:?} could not be reached! (ping)", conn.host_addr());
        return;
    }

    let mut f = File::open("package.zip").expect("failed to open package.zip");
    let mut buffer: Vec<u8> = Vec::new();
    f.read(&mut buffer).expect("failed to read package.zip");
    let res = conn.execute(ComputeRequest::new(buffer));
    if let Some(res) = res {
        println!("Result: {:?}", res);
    } else {
        println!("Host {:?} could not be reached! (compute)", conn.host_addr());
    }
}
