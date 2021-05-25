use std::fs;
use std::path::Path;
use bincode;
use karl_common::{Module, TarBuilder};

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

fn read_vec_string() -> Vec<String> {
    let mut tags = vec![];
    loop {
        let line = read_line();
        if line.is_empty() {
            return tags;
        }
        tags.push(line);
    }
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

fn main() {
    println!("Global module identifier:");
    let global_id = read_nonempty_line();
    println!("Files (one per line):");
    let builder = read_tar_builder();
    let handle = std::thread::spawn(|| builder.finalize().unwrap());
    println!("Invoke command:");
    let (binary_path, args) = read_invoke_command();
    println!("Input parameters (one per line):");
    let params = read_vec_string();
    println!("Output returns (one per line):");
    let returns = read_vec_string();
    println!("Requested network domains (one per line):");
    let network_perm = read_vec_string();

    println!("Done configuring! Building...");
    let package = handle.join().expect("failed to build tar");
    let module = Module {
        global_id,
        package,
        binary_path: Path::new(&binary_path).to_path_buf(),
        args,
        params,
        returns,
        network_perm,
    };

    let modules_path = std::env::var("KARL_MODULE_PATH").unwrap();
    let path = Path::new(&modules_path).join(&module.global_id);
    if path.exists() {
        println!("Path already exists. Override? [y/n]");
        if read_nonempty_line() != "y" {
            println!("Canceling.");
            return;
        }
    }
    println!("Writing module to {:?}", path);
    let bytes = bincode::serialize(&module).unwrap();
    println!("{} bytes", bytes.len());
    fs::write(&path, bytes).unwrap();
}
