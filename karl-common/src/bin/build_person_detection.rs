use std::fs;
use std::env;
use std::path::Path;
use bincode;
use karl_common::{Module, TarBuilder};

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
        "person_detection",
    ];
    let params = vec![
        vec!["image"],
    ];
    let returns = vec![
        vec!["box", "all_count", "count"],
    ];
    let network_perm = vec![
        vec!["metrics.com"],
    ];
    let package_lines = vec![
        vec![
            "person-detection/env env",
            "person-detection/torch torch",
            "person-detection/detect.py detect.py",
            "person-detection/karl.py karl.py",
            "person-detection/request_pb2.py request_pb2.py",
            "person-detection/request_pb2_grpc.py request_pb2_grpc.py",
        ],
    ];
    let args = vec![
        vec!["detect.py"],
    ];

    for i in 0..module_ids.len() {
        let global_id = module_ids[i].to_string();
        let params = params[i].iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let returns = returns[i].iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let network_perm = network_perm[i].iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let package = read_tar_builder(package_lines[i].iter().map(|line| {
            format!("{}/data/{}", env::var("KARL_PATH").unwrap(), line)
        }).collect()).finalize().unwrap();
        let binary_path = "./env/bin/python".to_string();
        let args = args[i].iter().map(|s| s.to_string()).collect::<Vec<_>>();

        let hook = Module {
            global_id,
            package,
            binary_path: Path::new(&binary_path).to_path_buf(),
            args,
            params,
            returns,
            network_perm,
        };

        let modules_path = std::env::var("KARL_MODULE_PATH").unwrap();
        let path = Path::new(&modules_path).join(&hook.global_id);
        println!("Writing hook to {:?}", path);
        let bytes = bincode::serialize(&hook).unwrap();
        println!("{} bytes", bytes.len());
        fs::write(&path, bytes).unwrap();
    }
}
