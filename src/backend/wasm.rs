use std::collections::HashSet;
use std::fs::File;
use std::path::Path;
use std::time::Instant;

use wasmer::executor::{Run, PkgConfig, replay_with_config};
use crate::ComputeResult;
use crate::{read_all, Error};

/// Run the compute request with the wasm backend.
///
/// Parameters:
/// - `config`: The compute config.
///    PkgConfig {
///        path,         # The path to a wasm binary.
///        mapped_dirs,  # Mapped directories based on import paths.
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
    // Replay the packaged computation.
    // Create the _compute_ working directory but stay in the karl path.
    let now = Instant::now();
    let mut options = Run::new(root_path.to_path_buf());
    let result = replay_with_config(&mut options, config)
        .expect("expected result");
    info!("=> execution: {} s", now.elapsed().as_secs_f32());

    // Return the requested results.
    let now = Instant::now();
    let mut res = ComputeResult::new();
    if res_stdout {
        res.stdout = result.stdout;
    }
    if res_stderr {
        res.stderr = result.stderr;
    }
    for path in res_files {
        let f = root_path.join(&path);
        match File::open(&f) {
            Ok(mut file) => {
                res.files.push((path, read_all(&mut file)?));
            },
            Err(e) => warn!("error opening output file {:?}: {:?}", f, e),
        }
    }
    info!("=> build result: {} s", now.elapsed().as_secs_f32());
    Ok(res)
}
