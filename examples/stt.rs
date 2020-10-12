#[macro_use]
extern crate log;

use std::time::{Duration, Instant};

use tokio::runtime::Runtime;
use karl::{net::Controller, *};

fn gen_request() -> ComputeRequest {
    let now = Instant::now();
    let request = ComputeRequestBuilder::new("stt/python")
        .args(vec![
            "stt/client.py",
            "--model",
            "stt/models.pbmm",
            "--scorer",
            "stt/models.scorer",
            "--audio",
            "stt/audio/2830-3980-0043.wav",
        ])
        .envs(vec!["PYTHONPATH=\
            stt/lib/python3.6/:\
            stt/lib/python3.6/lib-dynload:\
            stt/lib/python3.6/site-packages"])
        .build_root().unwrap()
        .add_dir("stt").unwrap()
        .finalize().unwrap();
    debug!("build request => {} s", now.elapsed().as_secs_f32());
    request
}

/// Requests computation from the host.
fn send(c: &mut Controller) -> Result<(), Error> {
    let start = Instant::now();
    let now = Instant::now();
    let request = gen_request().stdout();
    info!("build request: {} s", now.elapsed().as_secs_f32());

    let now = Instant::now();
    let handle = c.compute_async(request)?;
    info!("queue request: {} s", now.elapsed().as_secs_f32());
    let now = Instant::now();

    let result = c.rt.block_on(async { handle.await.unwrap() }).unwrap().stdout;
    info!("finished: {} s\n{:?}", now.elapsed().as_secs_f32(), result);
    info!("total: {} s", start.elapsed().as_secs_f32());
    Ok(())
}

fn main() {
    env_logger::builder().format_timestamp(None).init();
    let rt = Runtime::new().unwrap();
    let blocking = true;
    let mut c = Controller::new(rt, blocking);
    // Wait for the controller to add all hosts.
    std::thread::sleep(Duration::from_secs(5));
    send(&mut c).unwrap();
    info!("done.");
}
