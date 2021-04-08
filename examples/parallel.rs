#[macro_use]
extern crate log;
extern crate clap;

use std::time::Instant;
use clap::{Arg, App};
use tokio::runtime::Runtime;
use karl::{
    self,
    protos::ComputeRequest,
    common::{Error, ComputeRequestBuilder},
};

fn gen_request() -> ComputeRequest {
    let now = Instant::now();
    let request = ComputeRequestBuilder::new("data/add/python")
        .args(vec!["data/add/add.py", "20"])
        .envs(vec!["PYTHONPATH=data/add/lib/python3.6/"])
        .add_file("data/add/add.py")
        .add_file("data/add/python")
        .add_dir("data/add/lib/")
        .finalize()
        .unwrap();
    debug!("build request => {} s", now.elapsed().as_secs_f32());
    request
}

/// Requests computation from the host.
///
/// Parameters:
/// - controller - Controller <IP>:<PORT>.
/// - n - The total number of requests.
fn send_all(controller: &str, n: usize) -> Result<(), Error> {
    info!("registering client");
    let now = Instant::now();
    let res = karl::net::register_client(controller, "parallel", None);
    let client_token = res.get_client_token();
    info!("=> {} s", now.elapsed().as_secs_f32());

    let mut handles = vec![];
    let start = Instant::now();
    let now = Instant::now();
    let mut rt = Runtime::new().unwrap();
    let mut requests = vec![];
    for _ in 0..n {
        let request = gen_request();
        // request.mut_files().push("output.txt".to_string());
        requests.push(request);
    }
    info!("build {} requests: {} s", n, now.elapsed().as_secs_f32());
    let now = Instant::now();
    for mut request in requests.into_iter() {
        debug!("get host from controller");
        let now = Instant::now();
        let res = karl::net::get_host(controller, client_token, true);
        let host = format!("{}:{}", res.get_ip(), res.get_port());
        debug!("=> {} s ({:?})", now.elapsed().as_secs_f32(), host);
        debug!("send request");
        request.set_request_token(res.get_request_token().to_string());
        let handle = rt.spawn(async move {
            karl::net::send_compute(&host, request)
        });
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
            rt.block_on(async { handle.await })
        })
        .map(|result| result.unwrap())
        .map(|result| {
            let stdout_res = to_i64(&result.stdout);
            let mut file_res = None;
            for (path, bytes) in &result.files {
                if path == "output.txt" {
                    file_res = Some(to_i64(bytes));
                    break;
                }
            }
            (stdout_res, file_res.unwrap())
        })
        .collect::<Vec<_>>();
    info!("finished: {} s\n{:?}", now.elapsed().as_secs_f32(), results);
    info!("total: {} s", start.elapsed().as_secs_f32());
    Ok(())
}

fn main() {
    env_logger::builder().format_timestamp(None).init();
    let matches = App::new("Parallel Compute")
        .arg(Arg::with_name("host")
            .help("Host address of the controller")
            .short("h")
            .long("host")
            .takes_value(true)
            .default_value("127.0.0.1"))
        .arg(Arg::with_name("port")
            .help("Port of the controller")
            .short("p")
            .long("port")
            .takes_value(true)
            .default_value("59582"))
        .arg(Arg::with_name("n")
            .help("Number of parallel requests")
            .required(true))
        .get_matches();

    let n = matches.value_of("n").unwrap().parse::<usize>().unwrap();
    let host = matches.value_of("host").unwrap();
    let port = matches.value_of("port").unwrap();
    let addr = format!("{}:{}", host, port);
    send_all(&addr, n).unwrap();
    info!("done.");
}
