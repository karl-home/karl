#![feature(proc_macro_hygiene, decl_macro)]
#![feature(custom_inner_attributes)]

#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate rocket;
extern crate rocket_contrib;

mod dashboard;
pub mod net;
pub mod controller;
pub use controller::Controller;

pub mod protos {
	tonic::include_proto!("request");
}

use std::path::Path;
use clap::{Arg, App};
use tonic::transport::Server;
use protos::karl_controller_server::KarlControllerServer;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::builder().init();
    let matches = App::new("Controller")
        .arg(Arg::with_name("karl-path")
            .help("Absolute path to the base Karl directory.")
            .long("karl-path")
            .takes_value(true)
            .default_value("/home/gina/.karl_controller"))
        .arg(Arg::with_name("port")
            .help("Port. Defaults to a random open port.")
            .short("p")
            .long("port")
            .takes_value(true)
            .default_value("59582"))
        .arg(Arg::with_name("password")
            .help("Password required for a host to register with the controller.")
            .long("password")
            .takes_value(true)
            .default_value("password"))
        .arg(Arg::with_name("autoconfirm")
            .help("If the flag is included, automatically confirms all clients
                and hosts. Used for testing ONLY.")
            .long("autoconfirm"))
        .arg(Arg::with_name("dashboard")
            .help("If the flag is included, starts the dashboard.")
            .long("dashboard"))
        .arg(Arg::with_name("caching-enabled")
            .help("Whether caching is enabled (0 or 1)")
            .long("caching-enabled")
            .takes_value(true)
            .required(true))
        .arg(Arg::with_name("pubsub-enabled")
            .help("Whether the pubsub optimization is enabled (0 or 1)")
            .long("pubsub-enabled")
            .takes_value(true)
            .required(true))
        .get_matches();

    let port: u16 = matches.value_of("port").unwrap().parse().unwrap();
    let karl_path = Path::new(matches.value_of("karl-path").unwrap()).to_path_buf();
    let autoconfirm = matches.is_present("autoconfirm");
    let use_dashboard = matches.is_present("dashboard");
    let caching_enabled = matches.value_of("caching-enabled").unwrap() == "1";
    let pubsub_enabled = matches.value_of("pubsub-enabled").unwrap() == "1";
    let password = matches.value_of("password").unwrap();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut controller = Controller::new(
        rt.handle().clone(),
        karl_path,
        password,
        autoconfirm,
        caching_enabled,
        pubsub_enabled,
    );
    rt.block_on(async {
        controller.start(port).await.unwrap();
        if use_dashboard {
            dashboard::start(controller.clone());
        }
        Server::builder()
            .add_service(KarlControllerServer::new(controller))
            .serve(format!("0.0.0.0:{}", port).parse().unwrap())
            .await
            .unwrap();
    });
    Ok(())
}
