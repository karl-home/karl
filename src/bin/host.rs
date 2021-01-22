use std::path::Path;
use clap::{Arg, App};
use karl::Host;
use karl::backend::Backend;

fn main() {
    env_logger::builder().format_timestamp(None).init();
    let matches = App::new("Karl Host")
        .arg(Arg::with_name("karl-path")
            .help("Absolute path to the base Karl directory.")
            .long("karl-path")
            .takes_value(true)
            .default_value("/home/gina/.karl"))
        .arg(Arg::with_name("backend")
            .help("Host backend. Either 'wasm' for wasm executables or \
                `binary` for binary executables. Assumes macOS executables \
                only.")
            .short("b")
            .long("backend")
            .takes_value(true)
            .default_value("binary"))
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
    let karl_path = Path::new(matches.value_of("karl-path").unwrap()).to_path_buf();
    let port: u16 = matches.value_of("port").unwrap().parse().unwrap();
    let controller = format!(
        "{}:{}",
        matches.value_of("controller-ip").unwrap(),
        matches.value_of("controller-port").unwrap(),
    );
    let mut listener = Host::new(karl_path, backend, port, &controller);
    listener.start().unwrap();
}
