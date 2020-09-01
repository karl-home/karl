#[macro_use]
extern crate log;

use std::fs::File;
use std::time::Duration;

use tokio::runtime::Runtime;
use karl::{controller::Controller, *};

/// Pings the host. Returns whether it is a success.
fn ping(c: &mut Controller) {
    match c.ping() {
        Ok(Some(_)) => {},
        Ok(None) => warn!("could not be reached! (ping)"),
        Err(e) => error!("error pinging host: {:?}", e),
    }
}

/// Requests computation from the host.
fn compute(c: &mut Controller) {
    info!("reading package.zip");
    let mut f = File::open("package.zip").expect("failed to open package.zip");
    let buffer = read_packet(&mut f, false).expect("failed to read package.zip");
    info!("sending compute request");
    let request = ComputeRequest::new(buffer)
        .stdout()
        .stderr()
        .file("python/tmp2.txt");
    match c.execute(request) {
        Ok(Some(res)) => {
            info!("Result: {:?}", res);
            info!("stdout\n{}", String::from_utf8_lossy(&res.stdout));
            info!("stderr\n{}", String::from_utf8_lossy(&res.stderr));
        },
        Ok(None) => warn!("could not be reached! (compute)"),
        Err(e) => error!("error contacting host: {:?}", e),
    }
}

fn main() {
    env_logger::builder().format_timestamp(None).init();
    let rt = Runtime::new().unwrap();
    let blocking = true;
    let mut c = Controller::new(rt, blocking, Duration::from_secs(10));

    ping(&mut c);
    compute(&mut c);
}
