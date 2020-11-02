#[macro_use]
extern crate log;

use std::env;
use std::fs;
use std::process::{Command, Output};
use std::time::Instant;
use std::net::{TcpStream, TcpListener};

use clap::{Arg, App};
use karl::{self, Error};

fn run_cmd(
    bin: &str,
    envs: Vec<(String, String)>,
    args: Vec<String>,
) -> Output {
    debug!("binary => {:?}", bin);
    debug!("envs => {:?}", envs);
    debug!("args => {:?}", args);
    let mut cmd = Command::new(bin);
    for arg in args {
        cmd.arg(arg);
    }
    cmd.env_clear();
    for (key, val) in envs {
        cmd.env(key, val);
    }
    cmd.output().expect("failed to run process")
}

/// Run speech to text service given the path to an audio file.
///
/// Call scripts/setup_stt.sh before running the service to initialize
/// Python dependencies.
fn run_stt_python(audio_path: &str) -> Output {
    let home = env::var("HOME").unwrap();
    let stt_home = format!("{}/.karl/local/stt", home);
    let bin = format!("{}/bin/python", stt_home);
    let envs = vec![(
        "PYTHONPATH".to_string(),
        format!("{}/lib/python3.6:{}/lib/python3.6/lib-dynload:{}/lib/python3.6/site-packages", stt_home, stt_home, stt_home),
    )];
    let mut args = Vec::new();
    args.push(format!("{}/client.py", stt_home));
    args.push("--model".to_string());
    args.push(format!("{}/models.pbmm", stt_home));
    args.push("--scorer".to_string());
    args.push(format!("{}/models.scorer", stt_home));
    args.push("--audio".to_string());
    args.push(audio_path.to_string());
    run_cmd(&bin, envs, args)
}

fn run_stt_node(audio_path: &str) -> Output {
    let home = env::var("HOME").unwrap();
    let stt_home = format!("{}/.karl/local/stt_node", home);
    let bin = format!("{}/bin/node", stt_home);
    let envs = vec![];
    let mut args = Vec::new();
    args.push(format!("{}/main.js", stt_home));
    args.push(audio_path.to_string());
    run_cmd(&bin, envs, args)
}

/// Handle an incoming TCP stream.
fn handle_client(mut stream: TcpStream, mode: &str) -> Result<(), Error> {
    // Read the request from the TCP stream.
    let now = Instant::now();
    info!("reading packet");
    let buf = karl::read_packets(&mut stream, 1)?.remove(0);
    info!("=> {} s ({} bytes)", now.elapsed().as_secs_f32(), buf.len());

    // The bytes are just the audio file.
    info!("writing packet to file");
    let now = Instant::now();
    let home = env::var("HOME").unwrap();
    let path = format!("{}/.karl/audio.wav", home);
    fs::write(&path, buf)?;
    info!("=> {} s ({:?})", now.elapsed().as_secs_f32(), path);

    // Call the STT handler.
    info!("run stt");
    let now = Instant::now();
    let output = match mode {
        "python" => run_stt_python(&path),
        "node" => run_stt_node(&path),
        _ => unimplemented!(),
    };
    println!("{}", String::from_utf8_lossy(&output.stdout));
    println!("{}", String::from_utf8_lossy(&output.stderr));
    info!("=> {} s", now.elapsed().as_secs_f32());

    // Return the result to sender.
    info!("writing packet");
    let now = Instant::now();
    karl::write_packet(&mut stream, &output.stdout)?;
    info!("=> {} s", now.elapsed().as_secs_f32());
    Ok(())
}

fn main() {
    let matches = App::new("Standalone speech-to-text")
        .arg(Arg::with_name("mode")
            .help("Possible values: ['python', 'node'].")
            .short("m")
            .long("mode")
            .takes_value(true)
            .required(true))
        .get_matches();
    let mode = matches.value_of("mode").unwrap();
    assert!(mode == "python" || mode == "node");

    env_logger::builder().format_timestamp(None).init();
    let listener = TcpListener::bind("0.0.0.0:59582").unwrap();
    warn!("listening on port {}", listener.local_addr().unwrap().port());
    for stream in listener.incoming() {
        let stream = stream.unwrap();
        debug!("incoming stream {:?}", stream.local_addr());
        let now = Instant::now();
        if let Err(e) = handle_client(stream, mode) {
            error!("{:?}", e);
        }
        warn!("total: {} s", now.elapsed().as_secs_f32());
    }
}
