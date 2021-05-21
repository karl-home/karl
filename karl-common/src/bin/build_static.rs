use std::fs;
use std::path::Path;
use bincode;
use karl_common::{Module, HOOK_STORE_PATH, TarBuilder};

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
        "differential_privacy",
        "false",
        "firmware_update",
        "light_switch",
        "search",
        "targz",
        "true",
    ];

    let params = vec![
        vec!["count"],
        vec![],
        vec![],
        vec!["light_intent"],
        vec!["query_intent"],
        vec!["files"],
        vec![],
    ];

    let returns = vec![
        vec![],
        vec!["false"],
        vec!["firmware"],
        vec!["state"],
        vec!["response"],
        vec!["video"],
        vec!["true"],
    ];

    let network_perm = vec![
        vec!["metrics.com"],
        vec![],
        vec!["firmware.com"],
        vec![],
        vec!["google.com"],
        vec![],
        vec![],
    ];

    for i in 0..module_ids.len() {
        let global_id = module_ids[i].to_string();
        let params = params[i].iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let returns = returns[i].iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let network_perm = network_perm[i].iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let package = read_tar_builder(vec![
            format!("/home/gina/karl/karl-module-sdk/target/x86_64-unknown-linux-musl/release/examples/{} {}",
                &global_id, &global_id),
        ]).finalize().unwrap();
        let binary_path = format!("./{}", &global_id);
        let args = vec![];

        let hook = Module {
            global_id,
            package,
            binary_path: Path::new(&binary_path).to_path_buf(),
            args,
            params,
            returns,
            network_perm,
        };

        let path = Path::new(HOOK_STORE_PATH).join(&hook.global_id);
        println!("Writing hook to {:?}", path);
        let bytes = bincode::serialize(&hook).unwrap();
        println!("{} bytes", bytes.len());
        fs::write(&path, bytes).unwrap();
    }
}
