#[macro_use]
extern crate log;

use std::fs;
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use clap::{Arg, App};
use karl::{import::Import, net::Controller, *};

enum Mode {
    Standalone,
    KarlPython(bool),
    KarlNode(bool),
}

fn gen_request(mode: Mode, audio_file: &str) -> ComputeRequest {
    let now = Instant::now();
    let request = match mode {
        Mode::KarlPython(import) => gen_python_request(import, audio_file),
        Mode::KarlNode(import) => gen_node_request(import, audio_file),
        _ => unimplemented!(),
    };
    debug!("build request => {} s", now.elapsed().as_secs_f32());
    request
}

fn gen_python_request(import: bool, audio_file: &str) -> ComputeRequest {
    if import {
        ComputeRequestBuilder::new("python")
        .args(vec![
            "client.py",
            "--model",
            "models.pbmm",
            "--scorer",
            "models.scorer",
            "--audio",
            audio_file,
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
        .add_file(audio_file).unwrap()
        .finalize().unwrap()
    } else {
        ComputeRequestBuilder::new("stt/python")
        .args(vec![
            "data/stt/client.py",
            "--model",
            "data/stt/models.pbmm",
            "--scorer",
            "data/stt/models.scorer",
            "--audio",
            audio_file,
        ])
        .envs(vec!["PYTHONPATH=\
            data/stt/lib/python3.6/:\
            data/stt/lib/python3.6/lib-dynload:\
            data/stt/lib/python3.6/site-packages"])
        .build_root().unwrap()
        .add_file(audio_file).unwrap()
        .finalize().unwrap()
    }
}

fn gen_node_request(import: bool, audio_file: &str) -> ComputeRequest {
    if import {
        ComputeRequestBuilder::new("node")
        .args(vec![
            "main.js",
            audio_file,
            "models.pbmm",
            "models.scorer",
        ])
        .import(Import::Local {
            name: "stt_node".to_string(),
            hash: "TODO".to_string(),
        })
        .build_root().unwrap()
        .add_file(audio_file).unwrap()
        .finalize().unwrap()
    } else {
        unimplemented!();
    }
}

/// Requests computation from the host.
fn send(c: &mut Controller, mode: Mode, audio_file: &str) -> Result<(), Error> {
    let start = Instant::now();
    debug!("building request");
    let now = Instant::now();
    let request = gen_request(mode, audio_file).stdout();
    debug!("=> {} s", now.elapsed().as_secs_f32());

    let now = Instant::now();
    debug!("queue request");
    let handle = c.compute_async(request)?;
    debug!("=> {} s", now.elapsed().as_secs_f32());

    let now = Instant::now();
    let result = c.rt.block_on(async { handle.await.unwrap() }).unwrap().stdout;
    let result = String::from_utf8_lossy(&result);
    debug!("finished: {} s\n{}", now.elapsed().as_secs_f32(), result);
    info!("total: {} s", start.elapsed().as_secs_f32());
    Ok(())
}

fn send_standalone_request(host: SocketAddr, audio_file: &str) {
    let start = Instant::now();
    debug!("connect...");
    let now = Instant::now();
    let mut stream = TcpStream::connect(&host).unwrap();
    debug!("=> {} s", now.elapsed().as_secs_f32());

    info!("sending {:?} to {:?}...", audio_file, stream.peer_addr());
    let now = Instant::now();
    let mut f = fs::File::open(audio_file).unwrap();
    let bytes = karl::read_all(&mut f).unwrap();
    debug!("=> {} s (read file {} bytes)", now.elapsed().as_secs_f32(), bytes.len());
    write_packet(&mut stream, HT_RAW_BYTES, &bytes).unwrap();
    debug!("=> {} s (write to stream)", now.elapsed().as_secs_f32());

    // Wait for the response.
    debug!("waiting for response...");
    let now = Instant::now();
    let (header, bytes) = &read_packets(&mut stream, 1).unwrap()[0];
    assert_eq!(header.ty, HT_RAW_BYTES);
    debug!("=> {} s (read from stream)", now.elapsed().as_secs_f32());
    debug!("stdout:\n{}", String::from_utf8_lossy(bytes));
    info!("total: {} s", start.elapsed().as_secs_f32());
}

fn main() {
    env_logger::builder().format_timestamp(None).init();
    let matches = App::new("Speech-to-text")
        .arg(Arg::with_name("mode")
            .help("Possible values: ['standalone', 'karl_python', 'karl_node']. \
                The 'standalone' mode indicates a standalone STT service. The \
                karl modes indicate a generic computation service based on \
                karl, either using the Python or NodeJS backend.")
            .short("m")
            .long("mode")
            .takes_value(true)
            .required(true))
        .arg(Arg::with_name("host")
            .help("Host address of the standalone STT service")
            .short("h")
            .long("host")
            .takes_value(true)
            .default_value("127.0.0.1"))
        .arg(Arg::with_name("port")
            .help("Port of the standalone STT service.")
            .short("p")
            .long("port")
            .takes_value(true)
            .default_value("59582"))
        .arg(Arg::with_name("audio")
            .help("Path to audio file. Suggested: default or data/stt_node/weather.wav")
            .short("a")
            .long("audio")
            .takes_value(true)
            .default_value("data/stt/audio/2830-3980-0043.wav"))
        .arg(Arg::with_name("import")
            .long("import")
            .help("Whether to send the request with a local STT import."))
        .get_matches();

    let import = matches.is_present("import");
    let mode = match matches.value_of("mode").unwrap() {
        "standalone" => Mode::Standalone,
        "karl_python" => Mode::KarlPython(import),
        "karl_node" => Mode::KarlNode(import),
        mode => unimplemented!("unimplemented mode: {}", mode),
    };

    let audio_file = matches.value_of("audio").unwrap();
    match mode {
        Mode::Standalone => {
            let host = matches.value_of("host").unwrap();
            let port = matches.value_of("port").unwrap();
            let addr = format!("{}:{}", host, port);
            let host: SocketAddr = addr.parse().expect("malformed host");
            send_standalone_request(host, audio_file);
        },
        Mode::KarlPython(_) | Mode::KarlNode(_) => {
            let blocking = true;
            let mut c = Controller::new(blocking);
            // Wait for the controller to add all hosts.
            std::thread::sleep(Duration::from_secs(5));
            send(&mut c, mode, audio_file).unwrap();
        },
    }
    info!("done.");
}
