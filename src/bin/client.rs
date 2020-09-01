#[macro_use]
extern crate log;

use std::fs::File;
use std::time::Duration;

use tokio::runtime::Runtime;
use karl::{controller::Controller, *};

/// Pings the host. Returns whether it is a success.
fn ping(c: &mut Controller) {
    match c.ping() {
        Ok(res) => info!("Result: {:?}", res),
        Err(e) => error!("error pinging host: {:?}", e),
    }
}

/// Requests computation from the host.
fn compute(c: &mut Controller) {
    info!("reading package.tar.gz");
    let mut f = File::open("package.tar.gz").expect("failed to open package.tar.gz");
    let buffer = read_packet(&mut f, false).expect("failed to read package.tar.gz");
    info!("sending compute request");
    let filename = "python/tmp2.txt";
    let request = ComputeRequest::new(buffer)
        .stdout()
        .stderr()
        .file(filename);
    match c.execute(request) {
        Ok(res) => {
            info!("Result: {:?}", res);
            info!("stdout\n{}", String::from_utf8_lossy(&res.stdout));
            info!("stderr\n{}", String::from_utf8_lossy(&res.stderr));
            info!("{}\n{:?}", filename, res.files.get(filename).map(|bytes| {
                String::from_utf8_lossy(&bytes)
            }));
        },
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
