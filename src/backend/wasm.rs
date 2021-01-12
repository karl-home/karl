use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use wasmer::executor::{Run, replay_with_config};
use crate::protos::ComputeResult;
use crate::common::Error;

/// Run the compute request with the wasm backend.
///
/// Parameters:
/// - `binary_path`: The path to a wasm binary.
/// - `mapped_dirs`: Mapped directories based on import paths.
/// - `args`: Arguments.
/// - `envs`: Environment variables.
/// - `root_path`: The path to the computation root. Contains the unpacked
///   and decompressed bytes of the compute request. Should be a directory
///   within the service base path `~/.karl/<id>`.
/// - `res_stdout`: Whether to include stdout in the result.
/// - `res_stderr`: Whether to include stderr in the result.
/// - `res_files`: Files to include in the result, if they exist.
pub fn run(
    binary_path: PathBuf,
    mapped_dirs: Vec<String>,
    args: Vec<String>,
    envs: Vec<String>,
    root_path: &Path,
    res_stdout: bool,
    res_stderr: bool,
    res_files: HashSet<String>,
) -> Result<ComputeResult, Error> {
    // Replay the packaged computation.
    // Create the _compute_ working directory but stay in the karl path.
    let now = Instant::now();
    let mut options = Run::new(root_path.to_path_buf());
    let result = replay_with_config(&mut options, binary_path, mapped_dirs, args, envs)
        .expect("expected result");
    info!("=> execution: {} s", now.elapsed().as_secs_f32());

    // Return the requested results.
    let now = Instant::now();
    let mut res = ComputeResult::default();
    if res_stdout {
        res.set_stdout(result.stdout);
    }
    if res_stderr {
        res.set_stderr(result.stderr);
    }
    for path in res_files {
        let f = root_path.join(&path);
        match fs::read(&f) {
            Ok(bytes) => { res.mut_files().insert(path, bytes); },
            Err(e) => warn!("error reading output file {:?}: {:?}", f, e),
        }
    }
    info!("=> build result: {} s", now.elapsed().as_secs_f32());
    Ok(res)
}
