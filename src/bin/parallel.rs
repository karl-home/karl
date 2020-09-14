#[macro_use]
extern crate log;
extern crate clap;

use std::time::Instant;

use clap::{Arg, App};
use tokio::runtime::Runtime;
use karl::{controller::Controller, *};

fn gen_request() -> Result<ComputeRequest, Error> {
    let now = Instant::now();
    let request = ComputeRequestBuilder::new("add/python.wasm")
        .args(vec!["add/add.py", "20"])
        .build_root()?
        .add_file("add/add.py")?
        .add_file("add/python.wasm")?
        .add_dir("add/lib/")?
        .finalize()?;
        // .import(Import::Wapm { name: "python", version: "0.1.0" })?
    debug!("build request => {} s", now.elapsed().as_secs_f32());
    Ok(request)
}


/// Requests computation from the host.
///
/// Parameters:
/// - c - Connection to a controller.
/// - n - The total number of requests.
fn send_all(c: &mut Controller, n: usize) -> Result<(), Error> {
    let mut handles = vec![];
    let start = Instant::now();
    let now = Instant::now();
    let mut requests = vec![];
    for _ in 0..n {
        requests.push(gen_request()?.stdout());
    }
    info!("build {} requests: {} s", n, now.elapsed().as_secs_f32());
    let now = Instant::now();
    for request in requests.into_iter() {
        let handle = c.execute_async(request)?;
        handles.push(handle);
    }
    info!("queue {} requests: {} s", n, now.elapsed().as_secs_f32());
    let now = Instant::now();
    let results = handles
        .into_iter()
        .enumerate()
        .map(|(i, handle)| {
            debug!("{}/{}", i, n);
            c.rt.block_on(async { handle.await.unwrap() })
        })
        .map(|result| result.unwrap().stdout)
        .map(|bytes| String::from_utf8_lossy(&bytes).trim().parse::<i64>().unwrap())
        .collect::<Vec<_>>();
    info!("finished: {} s\n{:?}", now.elapsed().as_secs_f32(), results);
    info!("total: {} s", start.elapsed().as_secs_f32());
    Ok(())
}

fn main() {
    env_logger::builder().format_timestamp(None).init();
    let rt = Runtime::new().unwrap();
    let matches = App::new("Parallel Compute")
        .arg(Arg::with_name("n")
            .help("Number of parallel requests")
            .required(true))
        .get_matches();

    let n = matches.value_of("n").unwrap().parse::<usize>().unwrap();
    let blocking = true;
    let mut c = Controller::new(rt, blocking);
    send_all(&mut c, n).unwrap();
    info!("done.");
}
