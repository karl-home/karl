use clap::{Arg, App};
use karl::Host;
use karl::backend::Backend;

fn main() {
    env_logger::builder().format_timestamp(None).init();
    let matches = App::new("Karl Host")
        .arg(Arg::with_name("backend")
            .help("Host backend. Either 'wasm' for wasm executables or \
                `binary` for binary executables. Assumes macOS executables \
                only.")
            .short("b")
            .long("backend")
            .takes_value(true)
            .default_value("wasm"))
        .arg(Arg::with_name("port")
            .help("Port. Defaults to a random open port.")
            .short("p")
            .long("port")
            .takes_value(true)
            .default_value("0"))
        .arg(Arg::with_name("controller-ip")
            .help("IP address of the controller")
            .long("controller-ip")
            .takes_value(true)
            .default_value("127.0.0.1"))
        .arg(Arg::with_name("controller-port")
            .help("Port of the controller")
            .long("controller-port")
            .takes_value(true)
            .default_value("59582"))
        .arg(Arg::with_name("no-register")
            .help("If the flag is included, does not automatically register \
                the service with DNS-SD. The default is to register.")
            .long("no-register"))
        .get_matches();

    let backend = match matches.value_of("backend").unwrap() {
        "wasm" => {
            #[cfg(not(feature = "wasm"))]
            unimplemented!("wasm feature not enabled");
            #[cfg(feature = "wasm")]
            Backend::Wasm
        },
        "binary" => Backend::Binary,
        backend => unimplemented!("unimplemented backend: {}", backend),
    };
    let port: u16 = matches.value_of("port").unwrap().parse().unwrap();
    let register = !matches.is_present("no-register");
    let controller = format!(
        "{}:{}",
        matches.value_of("controller-ip").unwrap(),
        matches.value_of("controller-port").unwrap(),
    );
    let mut listener = Host::new(backend, port, register, &controller);
    listener.start().unwrap();
}
