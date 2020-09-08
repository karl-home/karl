#[macro_use]
extern crate log;
extern crate clap;

use std::fs::File;
use std::time::Instant;

use clap::{Arg, App};
use tokio::runtime::Runtime;
use karl::{controller::Controller, *};

fn gen_request() -> Result<ComputeRequest, Error> {
    let now = Instant::now();
    let mut f = File::open("add.tar.gz").expect("failed to open add.tar.gz");
    let buffer = read_packet(&mut f, false).expect("failed to read add.tar.gz");
    let request = ComputeRequest::new(buffer);
    info!("build request => {} s", now.elapsed().as_secs_f32());
    Ok(request)
}


/// Requests computation from the host.
///
/// Parameters:
/// - c - Connection to a controller.
/// - n - The total number of requests.
fn send_all(c: &mut Controller, n: usize) -> Result<(), Error> {
    for i in 0..n {
        let request = gen_request()?.stdout();
        let now = Instant::now();
        match c.execute(request) {
            Ok(res) => {
                let res = String::from_utf8_lossy(&res.stdout);
                info!("{} => {}", i, res.trim());
            },
            Err(e) => error!("{:?}", e),
        }
        info!("execute request => {} s", now.elapsed().as_secs_f32());
    }
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
