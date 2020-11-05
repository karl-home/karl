use clap::{Arg, App};
use karl::net::Controller;

fn main() {
    env_logger::builder().format_timestamp(None).init();
    let matches = App::new("Controller")
        .arg(Arg::with_name("port")
            .help("Port. Defaults to a random open port.")
            .short("p")
            .long("port")
            .takes_value(true)
            .default_value("54297"))
        .arg(Arg::with_name("no-register")
            .help("If the flag is included, does not automatically register \
                the service with DNS-SD. The default is to register.")
            .long("no-register"))
        .get_matches();

    let port: u16 = matches.value_of("port").unwrap().parse().unwrap();
    let _register = !matches.is_present("no-register");
    let mut controller = Controller::new(true);
    controller.start(port).unwrap();
}
