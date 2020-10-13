use std::env;
use std::fs;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Instant;

use wasmer::executor::PkgConfig;
use crate::ComputeResult;
use crate::{read_all, Error};

fn run_cmd(bin: PathBuf, envs: Vec<String>, args: Vec<String>) -> Output {
    let now = Instant::now();
    let mut cmd = Command::new(bin);
    error!("check 1: {:?}", now.elapsed().as_secs_f32());
    for arg in args {
        cmd.arg(arg);
    }
    error!("check 2: {:?}", now.elapsed().as_secs_f32());
    cmd.env_clear();
    error!("check 3: {:?}", now.elapsed().as_secs_f32());
    for envvar in envs {
    	let mut envvar = envvar.split("=");
    	let key = envvar.next().unwrap();
    	let val = envvar.next().unwrap();
    	assert!(envvar.next().is_none());
        cmd.env(key, val);
    }
    error!("check 4: {:?}", now.elapsed().as_secs_f32());
    let res = cmd.output().expect("failed to run process");
    error!("check 5: {:?}", now.elapsed().as_secs_f32());
    res
}

/// Copies a directory from the old (mapped) directory to the new (root)
/// directory. The directory path is a relative path.
fn copy(path: &Path, old_dir: &Path, new_dir: &Path) {
    debug!("copy {:?} {:?} {:?}", path, old_dir, new_dir);
    assert!(path.is_relative());
    let abs_path = old_dir.join(path);
    assert!(abs_path.is_dir());
    for f in fs::read_dir(&abs_path).expect(&format!("{:?}", abs_path)) {
        let f = f.unwrap();
        let ext_path = path.join(f.file_name());
        let old_path = old_dir.join(&ext_path);
        let new_path = new_dir.join(&ext_path);
        if new_path.exists() {
            continue;
        }
        if old_path.is_dir() {
            let e = format!("{:?}", new_path);
            fs::create_dir_all(new_path).expect(&e);
            copy(&ext_path, old_dir, new_dir);
        } else {
            let e = format!("{:?} {:?}", old_path, new_path);
            fs::copy(old_path, new_path).expect(&e);
        }
    }
}

fn map_dirs(mapped_dirs: Vec<String>) -> Result<(), Error> {
    for mapped_dir in mapped_dirs {
        let mut mapped_dir = mapped_dir.split(":");
        let new_dir = Path::new(mapped_dir.next().unwrap());
        let old_dir = Path::new(mapped_dir.next().unwrap());
        assert!(mapped_dir.next().is_none());
        copy(Path::new("."), old_dir, new_dir);
    }
    Ok(())
}

/// Copy an old path to the new path.

/// Run the compute request with the wasm backend.
///
/// Parameters:
/// - `config`: The compute config.
///    PkgConfig {
///        path,         # The path to a binary, hopefully compatible with
///                      # the platform of the current device.
///        mapped_dirs,  # The implementation _copies_ the files inside the
///                      # original directory into the `root_path`. If there
///                      # is already a file in the `root_path` with the same
///                      # relative filename, the file is not copied. Earlier
///                      # mapped directories have priority over later ones.
///                      # Ideally, this is mapped in the syscall layer.
///        args,         # Arguments.
///        envs,         # Environment variables.
///    }
/// - `root_path`: The path to the computation root. Contains the unpacked
///   and decompressed bytes of the compute request. Should be a directory
///   within the service base path `~/.karl/<id>`.
/// - `res_stdout`: Whether to include stdout in the result.
/// - `res_stderr`: Whether to include stderr in the result.
/// - `res_files`: Files to include in the result, if they exist.
pub fn run(
    config: PkgConfig,
    root_path: &Path,
    res_stdout: bool,
    res_stderr: bool,
    res_files: HashSet<String>,
) -> Result<ComputeResult, Error> {
    error!("{:?}", config);
    error!("{:?}", root_path);
    env::set_current_dir(root_path).unwrap();
    let now = Instant::now();
    map_dirs(config.mapped_dirs)?;
    info!("=> mapped_dirs: {} s", now.elapsed().as_secs_f32());
    // let root_path = Path::new("/Users/gina/.karl/1234/root/");
    // env::set_current_dir(root_path).unwrap();

    let now = Instant::now();
    let binary_path = config.binary_path.expect("expected binary path");
    let output = run_cmd(binary_path, config.envs, config.args);
    info!("=> execution: {} s", now.elapsed().as_secs_f32());

    // Return the requested results.
    warn!("{}", String::from_utf8_lossy(&output.stdout));
    warn!("{}", String::from_utf8_lossy(&output.stderr));
    let now = Instant::now();
    let mut res = ComputeResult::new();
    if res_stdout {
        res.stdout = output.stdout;
    }
    if res_stderr {
        res.stderr = output.stderr;
    }
    for path in res_files {
        let f = root_path.join(&path);
        match fs::File::open(&f) {
            Ok(mut file) => {
                res.files.insert(path, read_all(&mut file)?);
            },
            Err(e) => warn!("error opening output file {:?}: {:?}", f, e),
        }
    }
    info!("=> build result: {} s", now.elapsed().as_secs_f32());
    Ok(res)
}
