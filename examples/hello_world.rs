#[macro_use]
extern crate log;

use std::time::Instant;
use clap::{Arg, App};
use karl;

/// Requests computation from the host.
///
/// Params:
/// - controller: <IP>:<PORT>
fn send(controller: &str) {
    debug!("registering client");
    let now = Instant::now();
    let result = karl::net::register_client(controller, "hello-world-client", None);
    let client_token = result.get_client_token();
    debug!("=> {} s", now.elapsed().as_secs_f32());

    debug!("registering hook");
    let now = Instant::now();
    karl::net::register_hook(controller, client_token, "hello-world");
    debug!("=> {} s", now.elapsed().as_secs_f32());
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
