use std::path::Path;
use clap::{Arg, App};
use karl::Controller;

fn main() {
    env_logger::builder().format_timestamp(None).init();
    let matches = App::new("Controller")
        .arg(Arg::with_name("karl-path")
            .help("Absolute path to the base Karl directory.")
            .long("karl-path")
            .takes_value(true)
            .default_value("/home/gina/.karl"))
        .arg(Arg::with_name("port")
            .help("Port. Defaults to a random open port.")
            .short("p")
            .long("port")
            .takes_value(true)
            .default_value("59582"))
        .arg(Arg::with_name("no-register")
            .help("If the flag is included, does not automatically register \
                the service with DNS-SD. The default is to register.")
            .long("no-register"))
        .arg(Arg::with_name("no-dashboard")
            .help("If the flag is included, does not automatically start a \
                controller dashboard. The default is to start a dashbaord.")
            .long("no-dashboard"))
        .get_matches();

    let port: u16 = matches.value_of("port").unwrap().parse().unwrap();
    let karl_path = Path::new(matches.value_of("karl-path").unwrap()).to_path_buf();
    let _register = !matches.is_present("no-register");
    let use_dashboard = !matches.is_present("no-dashboard");
    let mut controller = Controller::new(karl_path);
    controller.start(use_dashboard, port).unwrap();
}
