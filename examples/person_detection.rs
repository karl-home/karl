#[macro_use]
extern crate log;

use std::time::Instant;

use clap::{Arg, App};
use karl::{self, common::ComputeRequestBuilder, protos::{Import, ComputeRequest}};

static CLIENT_ID: &str = "camera";

fn gen_request(import: bool, img_path: &str) -> ComputeRequest {
    let model_path = "torch/checkpoints/maskrcnn_resnet50_fpn_coco-bf2d0c1e.pth";
    let dst_img_path = std::path::Path::new(img_path)
        .file_name().unwrap()
        .to_str().unwrap();
    let mut request = ComputeRequestBuilder::new("env/bin/python")
        .args(vec!["detect.py", dst_img_path])
        .add_file("data/person-detection/detect.py", "detect.py")
        .add_file(img_path, dst_img_path);
    request = if import {
        request
        .import(Import {
            name: "person-detection".to_string(),
            hash: "TODO".to_string(),
            ..Default::default()
        })
    } else {
        request
        .add_file(&format!("data/person-detection/{}", model_path), model_path)
        .add_dir("data/person-detection/env/", "env/")
    };
    request.finalize().unwrap()
}

/// Requests computation from the host.
///
/// Params:
/// - controller: <IP>:<PORT>
/// - storage: whether to request persistent storage
/// - import: whether to build the request with an import
/// - img_path: src path to image
fn send(controller: &str, storage: bool, import: bool, img_path: &str) {
    let start = Instant::now();
    debug!("building request");
    let now = Instant::now();
    let mut request = gen_request(import, img_path);
    request.set_stdout(true);
    request.set_client_id(CLIENT_ID.to_string());
    if storage {
        request.set_storage(true);
    }
    debug!("=> {} s", now.elapsed().as_secs_f32());

    debug!("get host from controller");
    let now = Instant::now();
    let host = karl::net::get_host(controller);
    debug!("=> {} s ({:?})", now.elapsed().as_secs_f32(), host);

    let now = Instant::now();
    debug!("send request");
    let result = karl::net::send_compute(&host, request);
    let result = String::from_utf8_lossy(&result.stdout);
    debug!("finished: {} s\n{}", now.elapsed().as_secs_f32(), result);
    info!("total: {} s", start.elapsed().as_secs_f32());
}

/// Registers a client with id CLIENT_ID with the controller.
fn register(controller: &str) {
    info!("registering client id {:?} with controller", CLIENT_ID);
    karl::net::register_client(controller, CLIENT_ID);
}

fn main() {
    env_logger::builder().format_timestamp(None).init();
    let matches = App::new("Speech-to-text")
        .arg(Arg::with_name("host")
            .help("Host address of the standalone STT service / controller")
            .short("h")
            .long("host")
            .takes_value(true)
            .default_value("127.0.0.1"))
        .arg(Arg::with_name("port")
            .help("Port of the standalone STT service / controller")
            .short("p")
            .long("port")
            .takes_value(true)
            .default_value("59582"))
        .arg(Arg::with_name("img")
             .help("Src path of image to detect.")
             .short("i")
             .long("img")
             .takes_value(true)
             .default_value("data/person-detection/PennFudanPed/PNGImages/FudanPed00001.png"))
        .arg(Arg::with_name("import")
             .long("import")
             .help("Whether to send the request with a person-detection import."))
        .arg(Arg::with_name("storage")
             .long("storage")
             .help("Whether to request persistent storage."))
        .get_matches();

    let host = matches.value_of("host").unwrap();
    let port = matches.value_of("port").unwrap();
    let addr = format!("{}:{}", host, port);
    let img_path = matches.value_of("img").unwrap();
    let import = matches.is_present("import");
    let storage = matches.is_present("storage");
    register(&addr);
    send(&addr, storage, import, img_path);
    info!("done.");
}
