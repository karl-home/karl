use std::fs;
use std::path::Path;
use bincode;
use regex::Regex;
use karl::hook::{Hook, HookSchedule, FileACL, HOOK_STORE_PATH};
use karl::common::TarBuilder;

fn read_nonempty_line() -> String {
    loop {
        let line = read_line();
        if !line.is_empty() {
            // println!("read_nonempty_line(): {:?}", line);
            return line;
        }
    }
}

fn read_line() -> String {
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).unwrap();
    let line = line.trim().to_string();
    // println!("read_line(): {:?}", line);
    line
}

fn read_state_perm() -> Vec<String> {
    let mut state_perm = vec![];
    loop {
        let line = read_line();
        if line.is_empty() {
            break;
        }
        state_perm.push(line);
    }
    state_perm
}

fn read_network_perm() -> Vec<String> {
    let mut network_perm = vec![];
    let re = Regex::new(r"https://[a-z0-9]+(\.[a-z0-9]+)+").unwrap();
    loop {
        let line = read_line();
        if line.is_empty() {
            break;
        }
        if !re.is_match(&line) {
            println!("Invalid domain - https://[a-z0-9]+(.[a-z0-9]+)+");
        } else {
            network_perm.push(line);
        }
    }
    network_perm
}

fn read_file_perm() -> Vec<FileACL> {
    let mut file_perm = vec![];
    let re = Regex::new(r"(?P<path>\S+) (?P<read>r)?(?P<write>w)?").unwrap();
    loop {
        let line = read_line();
        if line.is_empty() {
            break;
        }
        if let Some(captures) = re.captures(&line) {
            let path = &captures.name("path").unwrap().as_str();
            let read = captures.name("read").is_some();
            let write = captures.name("write").is_some();
            if read || write {
                file_perm.push(FileACL {
                    path: Path::new(path).to_path_buf(),
                    read,
                    write,
                });
            } else {
                println!("At least one of read/write perm required");
            }
        } else {
            println!("Invalid format - <path> (r|w|rw)");
        }
    }
    file_perm
}

fn read_tar_builder() -> TarBuilder {
    let mut builder = TarBuilder::new();
    loop {
        let line = read_line();
        if line.is_empty() {
            break;
        }
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

fn read_invoke_command() -> (String, Vec<String>) {
    let line = read_nonempty_line();
    let args: Vec<&str> = line.split(" ").collect();
    let mut args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let binary_path = args.remove(0);
    (binary_path, args)
}

fn read_envs() -> Vec<(String, String)> {
    let mut envs = vec![];
    loop {
        let line = read_line();
        if line.is_empty() {
            break;
        }
        let line: Vec<&str> = line.split(" ").collect();
        if line.len() != 2 {
            println!("Invalid format - space-separated '<KEY> <VALUE>'");
        } else {
            envs.push((line[0].to_string(), line[1].to_string()));
        }
    }
    envs
}

fn main() {
    env_logger::builder().format_timestamp(None).init();

    println!("Global hook identifier:");
    let global_hook_id = read_nonempty_line();
    println!("Files (one per line):");
    let builder = read_tar_builder();
    let handle = std::thread::spawn(|| builder.finalize().unwrap());
    println!("Invoke command:");
    let (binary_path, args) = read_invoke_command();
    println!("Environment variables (one per line):");
    let envs = read_envs();
    println!("Interval (i) or watch tag (w)?:");
    let schedule = loop {
        let line = read_nonempty_line();
        if line == "i".to_string() {
            println!("Time (spawned at this interval, in seconds):");
            let secs: usize = read_nonempty_line().parse().unwrap();
            let duration = std::time::Duration::from_secs(secs as _);
            break HookSchedule::Interval(duration);
        } else if line == "w".to_string() {
            println!("Tag (spawned when data is pushed here):");
            let tag = read_nonempty_line();
            break HookSchedule::WatchTag(tag);
        }
    };
    println!("Permitted sensor IDs to change state (one per line):");
    let state_perm = read_state_perm();
    println!("Permitted network domains (one per line):");
    let network_perm = read_network_perm();
    println!("Permitted file ACLs (one per line):");
    let file_perm = read_file_perm();

    println!("Done configuring! Building...");
    let package = handle.join().expect("failed to build tar");
    let hook = Hook::new(
        global_hook_id,
        schedule,
        state_perm,
        network_perm,
        file_perm,
        package,
        &binary_path,
        args,
        envs,
    );
    println!("");
    println!("global_hook_id: {:?}", hook.global_hook_id);
    println!("package: {:?} bytes", hook.package.len());
    println!("binary_path: {:?}", hook.binary_path);
    println!("args: {:?}", hook.args);
    println!("envs: {:?}", hook.envs);
    println!("schedule: {:?}", hook.schedule);
    println!("state_perm: {:?}", hook.state_perm);
    println!("network_perm: {:?}", hook.network_perm);
    println!("file_perm: {:?}", hook.file_perm);

    println!("Confirm? [y/n]");
    if read_nonempty_line() != "y" {
        println!("Canceling.");
    } else {
        let path = Path::new(HOOK_STORE_PATH).join(&hook.global_hook_id);
        println!("Writing hook to {:?}", path);
        let bytes = bincode::serialize(&hook).unwrap();
        println!("{} bytes", bytes.len());
        fs::write(&path, bytes).unwrap();
    }
}
