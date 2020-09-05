#[macro_use]
extern crate log;

use std::time::Instant;

use tokio::runtime::Runtime;
use karl::{controller::Controller, *};

/// Pings the host. Returns whether it is a success.
fn ping(c: &mut Controller) {
    info!("sending ping request");
    let now = Instant::now();
    match c.ping() {
        Ok(res) => info!("Result: {:?}", res),
        Err(e) => error!("error pinging host: {:?}", e),
    }
    info!("=> {} s", now.elapsed().as_secs_f32());
}

/// Requests computation from the host.
fn compute(c: &mut Controller) -> Result<(), Error> {
    info!("building compute request");
    let now = Instant::now();
    let request = ComputeRequestBuilder::new("python/python.wasm")
        .preopen_dirs(vec!["python/"])
        .args(vec!["python/run.py"])
        .build_root()?
        .add_dir("python/")?
        .finalize()?;
    // let mut f = File::open("package.tar.gz").expect("failed to open package.tar.gz");
    // let buffer = read_packet(&mut f, false).expect("failed to read package.tar.gz");
    // let request = ComputeRequest::new(buffer);
    info!("=> {} s", now.elapsed().as_secs_f32());

    info!("sending compute request");
    let now = Instant::now();
    let filename = "python/tmp2.txt";
    let request = request
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
    info!("=> {} s", now.elapsed().as_secs_f32());
    Ok(())
}

fn main() {
    env_logger::builder().format_timestamp(None).init();
    let rt = Runtime::new().unwrap();
    let blocking = true;
    let mut c = Controller::new(rt, blocking);

    ping(&mut c);
    compute(&mut c).unwrap();
}
