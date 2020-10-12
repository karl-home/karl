use std::env;
use std::collections::HashSet;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Instant;

use wasmer::executor::PkgConfig;
use crate::ComputeResult;
use crate::{read_all, Error};

fn run_cmd(bin: PathBuf, envs: Vec<String>, args: Vec<String>) -> Output {
    let mut cmd = Command::new(bin);
    for arg in args {
        cmd.arg(arg);
    }
    cmd.env_clear();
    for envvar in envs {
    	let mut envvar = envvar.split("=");
    	let key = envvar.next().unwrap();
    	let val = envvar.next().unwrap();
    	assert!(envvar.next().is_none());
        cmd.env(key, val);
    }
    cmd.output().expect("failed to run process")
}

/// Run the compute request with the wasm backend.
///
/// Parameters:
/// - `config`: The compute config.
///    PkgConfig {
///        path,         # The path to a binary, hopefully compatible with
///                      # the platform of the current device.
///        mapped_dirs,  # WARNING: NOT USED.
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
    let now = Instant::now();
    env::set_current_dir(root_path).unwrap();
    let binary_path = config.binary_path.expect("expected binary path");
    let output = run_cmd(binary_path, config.envs, config.args);
    info!("=> execution: {} s", now.elapsed().as_secs_f32());

    // Return the requested results.
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
        match File::open(&f) {
            Ok(mut file) => {
                res.files.insert(path, read_all(&mut file)?);
            },
            Err(e) => warn!("error opening output file {:?}: {:?}", f, e),
        }
    }
    info!("=> build result: {} s", now.elapsed().as_secs_f32());
    Ok(res)
}
