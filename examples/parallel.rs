#[macro_use]
extern crate log;
extern crate clap;

use std::time::{Duration, Instant};

use clap::{Arg, App};
use tokio::runtime::Runtime;
use karl::{backend::Backend, net::Controller, import::Import, *};

fn gen_request(backend: &Backend) -> ComputeRequest {
    let now = Instant::now();
    let request = match backend {
        Backend::Wasm => {
            ComputeRequestBuilder::new("add/python.wasm")
                .args(vec!["add/add.py", "20"])
                .import(Import::Wapm {
                    name: "python".to_string(),
                    version: "0.1.0".to_string(),
                })
                .build_root().unwrap()
                .add_file("add/add.py").unwrap()
                .finalize().unwrap()
        },
        Backend::Binary => {
            ComputeRequestBuilder::new("add/python")
                .args(vec!["add/add.py", "20"])
                .envs(vec!["PYTHONPATH=add/lib/python3.6/"])
                .build_root().unwrap()
                .add_file("add/add.py").unwrap()
                .add_file("add/python").unwrap()
                .add_dir("add/lib/").unwrap()
                .finalize().unwrap()
        },
    };
    debug!("build request => {} s", now.elapsed().as_secs_f32());
    request
}

/// Requests computation from the host.
///
/// Parameters:
/// - c - Connection to a controller.
/// - n - The total number of requests.
fn send_all(c: &mut Controller, n: usize, backend: &Backend) -> Result<(), Error> {
    let mut handles = vec![];
    let start = Instant::now();
    let now = Instant::now();
    let mut requests = vec![];
    for _ in 0..n {
        requests.push(gen_request(backend).stdout().file("add/output.txt"));
    }
    info!("build {} requests: {} s", n, now.elapsed().as_secs_f32());
    let now = Instant::now();
    for request in requests.into_iter() {
        let handle = c.compute_async(request)?;
        handles.push(handle);
    }
    info!("queue {} requests: {} s", n, now.elapsed().as_secs_f32());
    let now = Instant::now();
    let to_i64 = |bytes: &Vec<u8>| {
        String::from_utf8_lossy(bytes).trim().parse::<i64>().unwrap()
    };
    let results = handles
        .into_iter()
        .enumerate()
        .map(|(i, handle)| {
            debug!("{}/{}", i, n);
            c.rt.block_on(async { handle.await.unwrap() })
        })
        .map(|result| result.unwrap())
        .map(|result| (
            to_i64(&result.stdout),
            result.files.get("add/output.txt").map(to_i64),
        ))
        .collect::<Vec<_>>();
    info!("finished: {} s\n{:?}", now.elapsed().as_secs_f32(), results);
    info!("total: {} s", start.elapsed().as_secs_f32());
    Ok(())
}

fn main() {
    env_logger::builder().format_timestamp(None).init();
    let rt = Runtime::new().unwrap();
    let matches = App::new("Parallel Compute")
        .arg(Arg::with_name("backend")
            .help("Service backend. Either 'wasm' for wasm executables or \
                `binary` for binary executables. Assumes macOS executables \
                only.")
            .short("b")
            .long("backend")
            .takes_value(true)
            .default_value("wasm"))
        .arg(Arg::with_name("n")
            .help("Number of parallel requests")
            .required(true))
        .get_matches();

    let n = matches.value_of("n").unwrap().parse::<usize>().unwrap();
    let backend = match matches.value_of("backend").unwrap() {
        "wasm" => Backend::Wasm,
        "binary" => Backend::Binary,
        backend => unimplemented!("unimplemented backend: {}", backend),
    };
    let blocking = true;
    let mut c = Controller::new(rt, blocking);
    // Wait for the controller to add all hosts.
    std::thread::sleep(Duration::from_secs(5));
    send_all(&mut c, n, &backend).unwrap();
    info!("done.");
}
