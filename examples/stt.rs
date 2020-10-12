#[macro_use]
extern crate log;

use std::time::{Duration, Instant};

use clap::{Arg, App};
use tokio::runtime::Runtime;
use karl::{import::Import, net::Controller, *};

fn gen_request(import: bool) -> ComputeRequest {
    let now = Instant::now();
    let request = if import {
        ComputeRequestBuilder::new("python")
        .args(vec![
            "client.py",
            "--model",
            "models.pbmm",
            "--scorer",
            "models.scorer",
            "--audio",
            "audio/2830-3980-0043.wav",
        ])
        .envs(vec!["PYTHONPATH=\
            lib/python3.6/:\
            lib/python3.6/lib-dynload:\
            lib/python3.6/site-packages"])
        .import(Import::Local {
            name: "stt".to_string(),
            hash: "TODO".to_string(),
        })
        .build_root().unwrap()
        .add_dir("audio/").unwrap()
        .finalize().unwrap()
    } else {
        ComputeRequestBuilder::new("stt/python")
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
        .finalize().unwrap()
    };
    debug!("build request => {} s", now.elapsed().as_secs_f32());
    request
}

/// Requests computation from the host.
fn send(c: &mut Controller, import: bool) -> Result<(), Error> {
    let start = Instant::now();
    info!("building request");
    let now = Instant::now();
    let request = gen_request(import).stdout();
    info!("=> {} s", now.elapsed().as_secs_f32());

    let now = Instant::now();
    info!("queue request");
    let handle = c.compute_async(request)?;
    info!("=> {} s", now.elapsed().as_secs_f32());
    let now = Instant::now();

    let result = c.rt.block_on(async { handle.await.unwrap() }).unwrap().stdout;
    let result = String::from_utf8_lossy(&result);
    info!("finished: {} s\n{}", now.elapsed().as_secs_f32(), result);
    info!("total: {} s", start.elapsed().as_secs_f32());
    Ok(())
}

fn main() {
    env_logger::builder().format_timestamp(None).init();
    let matches = App::new("Speech-to-text")
        .arg(Arg::with_name("import")
            .long("import")
            .help("Whether to send the request with a local STT import."))
        .get_matches();

    let rt = Runtime::new().unwrap();
    let import = matches.is_present("import");
    let blocking = true;
    let mut c = Controller::new(rt, blocking);
    // Wait for the controller to add all hosts.
    std::thread::sleep(Duration::from_secs(5));
    send(&mut c, import).unwrap();
    info!("done.");
}
