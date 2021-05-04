#![feature(proc_macro_hygiene, decl_macro)]
#![feature(custom_inner_attributes)]

#[macro_use]
extern crate log;
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
        .get_matches();

    let port: u16 = matches.value_of("port").unwrap().parse().unwrap();
    let karl_path = Path::new(matches.value_of("karl-path").unwrap()).to_path_buf();
    let autoconfirm = matches.is_present("autoconfirm");
    let dashboard = matches.is_present("dashboard");
    let password = matches.value_of("password").unwrap();
    let mut controller = Controller::new(karl_path, password, autoconfirm);
    controller.start(dashboard, port).await.unwrap();
    Server::builder()
        .add_service(KarlControllerServer::new(controller))
        .serve(format!("0.0.0.0:{}", port).parse()?)
        .await
        .unwrap();
    Ok(())
}
