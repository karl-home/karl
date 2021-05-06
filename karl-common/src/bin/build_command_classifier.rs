use std::fs;
use std::path::Path;
use bincode;
use karl_common::{Hook, HOOK_STORE_PATH, TarBuilder};

fn read_tar_builder(lines: Vec<String>) -> TarBuilder {
    let mut builder = TarBuilder::new();
    for line in lines {
        let line = line.split(" ").collect::<Vec<_>>();
        let path = std::path::Path::new(line[0]);
        if !path.exists() {
            println!("Path does not exist");
            continue;
        }
        let is_dir = path.is_dir();
        if line.len() == 1 {
            if is_dir {
                builder = builder.add_dir(line[0]);
            } else {
                builder = builder.add_file(line[0]);
            }
        } else if line.len() == 2 {
            if is_dir {
                builder = builder.add_dir_as(line[0], line[1]);
            } else {
                builder = builder.add_file_as(line[0], line[1]);
            }
        } else {
            println!("'<path>' or '<old_path> <new_path>'");
            continue;
        }
    }
    builder
}

fn main() {
    let module_ids = vec![
        "command_classifier",
        "command_classifier_bulb",
        "command_classifier_search",
    ];
    let params = vec![
        vec!["sound"],
        vec!["sound"],
        vec!["sound"],
    ];
    let returns = vec![
        vec!["light", "search"],
        vec!["light", "search"],
        vec!["light", "search"],
    ];
    let package_lines = vec![
        vec![
            "picovoice/env env",
            "picovoice/picovoice_demo_file.py picovoice_demo_file.py",
            "picovoice/picovoice_linux.ppn picovoice_linux.ppn",
            "picovoice/coffee_maker_linux.rhn coffee_maker_linux.rhn",
            "picovoice/libsndfile.so.1 libsndfile.so.1",
            "picovoice/request_pb2_grpc.py request_pb2_grpc.py",
            "picovoice/request_pb2.py request_pb2.py",
            "picovoice/karl.py karl.py",
        ],
        vec![
            "picovoice/env env",
            "picovoice/picovoice_bulb.py picovoice_bulb.py",
            "picovoice/picovoice_linux.ppn picovoice_linux.ppn",
            "picovoice/coffee_maker_linux.rhn coffee_maker_linux.rhn",
            "picovoice/libsndfile.so.1 libsndfile.so.1",
            "picovoice/request_pb2_grpc.py request_pb2_grpc.py",
            "picovoice/request_pb2.py request_pb2.py",
            "picovoice/karl.py karl.py",
        ],
        vec![
            "picovoice/env env",
            "picovoice/picovoice_search.py picovoice_search.py",
            "picovoice/picovoice_linux.ppn picovoice_linux.ppn",
            "picovoice/coffee_maker_linux.rhn coffee_maker_linux.rhn",
            "picovoice/libsndfile.so.1 libsndfile.so.1",
            "picovoice/request_pb2_grpc.py request_pb2_grpc.py",
            "picovoice/request_pb2.py request_pb2.py",
            "picovoice/karl.py karl.py",
        ],
    ];
    let args = vec![
        vec!["picovoice_demo_file.py"],
        vec!["picovoice_bulb.py"],
        vec!["picovoice_search.py"],
    ];

    for i in 0..module_ids.len() {
        let global_hook_id = module_ids[i].to_string();
        let params = params[i].iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let returns = returns[i].iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let package = read_tar_builder(package_lines[i].iter().map(|line| {
            format!("/home/gina/karl/data/{}", line)
        }).collect()).finalize().unwrap();
        let binary_path = "./env/bin/python".to_string();
        let args = args[i].iter().map(|s| s.to_string()).collect::<Vec<_>>();

        let hook = Hook::new(
            global_hook_id,
            package,
            &binary_path,
            args,
            params,
            returns,
        );

        let path = Path::new(HOOK_STORE_PATH).join(&hook.global_hook_id);
        println!("Writing hook to {:?}", path);
        let bytes = bincode::serialize(&hook).unwrap();
        println!("{} bytes", bytes.len());
        fs::write(&path, bytes).unwrap();
    }
}
