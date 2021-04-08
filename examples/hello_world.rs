#[macro_use]
extern crate log;

use std::time::Instant;

use clap::{Arg, App};
use karl::{self, common::ComputeRequestBuilder, protos::ComputeRequest};

fn gen_request() -> ComputeRequest {
    ComputeRequestBuilder::new("env/bin/python")
        .args(vec!["main.py"])
        .add_dir_as("data/hello_world/env/", "env/")
        .add_file_as("data/hello_world/main.py", "main.py")
        .finalize()
        .unwrap()
}

/// Requests computation from the host.
///
/// Params:
/// - controller: <IP>:<PORT>
fn send(controller: &str) {
    let start = Instant::now();
    debug!("registering client");
    let now = Instant::now();
    let result = karl::net::register_client(controller, "hello_world", None);
    let client_token = result.get_client_token();
    debug!("=> {} s", now.elapsed().as_secs_f32());

    debug!("building request");
    let now = Instant::now();
    let mut request = gen_request();
    debug!("=> {} s", now.elapsed().as_secs_f32());

    debug!("get host from controller");
    let now = Instant::now();
    let res = karl::net::get_host(controller, client_token, true);
    let host = format!("{}:{}", res.get_ip(), res.get_port());
    let request_token = res.get_request_token();
    debug!("=> {} s ({:?})", now.elapsed().as_secs_f32(), host);

    let now = Instant::now();
    debug!("send request");
    request.set_request_token(request_token.to_string());
    let result = karl::net::send_compute(&host, request);
    let result = String::from_utf8_lossy(&result.stdout);
    debug!("finished: {} s\n{}", now.elapsed().as_secs_f32(), result);
    info!("total: {} s", start.elapsed().as_secs_f32());
}

fn main() {
    env_logger::builder().format_timestamp(None).init();
    let matches = App::new("Speech-to-text")
        .arg(Arg::with_name("ip")
            .help("Controller ip.")
            .long("ip")
            .takes_value(true)
            .default_value("127.0.0.1"))
        .arg(Arg::with_name("port")
            .help("Controller port.")
            .short("p")
            .long("port")
            .takes_value(true)
            .default_value("59582"))
        .get_matches();

    let ip = matches.value_of("ip").unwrap();
    let port = matches.value_of("port").unwrap();
    let addr = format!("{}:{}", ip, port);
    send(&addr);
    info!("done.");
}
