//! Code for executing a process inside a host.
//!
//! Runtime for arbitrary Linux executables that mounts the appropriate
//! directories, configures environment variables, and otherwise manages
//! the sandbox for offloading compute requests.
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Instant;

use karl_common::Error;

fn run_cmd(
    root_path: &Path,
    bin: PathBuf,
    envs: Vec<String>,
    args: Vec<String>,
) -> Output {
    trace!("bin: {:?}", bin);
    trace!("envs: {:?}", envs);
    trace!("args: {:?}", args);

    env::set_current_dir(root_path).unwrap();
    let mut cmd = Command::new("firejail");
    cmd.arg("--quiet");
    cmd.arg(format!("--private={}", root_path.to_str().unwrap()));
    cmd.arg("--netfilter=/etc/firejail/karl.net");
    for env in envs {
        cmd.arg(format!("--env={}", env));
    }
    cmd.arg(bin);
    for arg in args {
        cmd.arg(arg);
    }
    cmd.env_clear();
    debug!("{:?}", cmd);
    cmd.output().expect("failed to run process")
}

/*
/// Adds a symlink `storage` to the root path and links to a directory
/// containing the particularly client's persistent storage.
///
/// The persistent storage path is found at `<karl_path>/storage/<client_id>`.
/// Creates the persistent storage path if it does not already exist.
/// There must not already be a file at `<root_path>/storage`.
///
/// Parameters:
/// - karl_path - Probably ~/.karl.
/// - root_path - The initial filesystem provided by the client, and the
///   eventual working directory.
/// - client_id - The ID of a registered client / IoT device.
#[cfg(target_os = "linux")]
fn symlink_storage(
    karl_path: &Path,
    root_path: &Path,
    client_id: &str,
) -> Result<(), Error> {
    let storage_path = karl_path.join("storage").join(client_id);
    let target_path = root_path.join("storage");
    debug!("creating symlink from {:?} to {:?}", storage_path, target_path);

    if storage_path.exists() && !storage_path.is_dir() {
        return Err(Error::StorageError(format!("storage path is not a dir")));
    }
    if target_path.exists() {
        return Err(Error::StorageError(format!("target storage path already exists")));
    }
    fs::create_dir_all(&storage_path).map_err(|e| Error::StorageError(
        format!("Failed to initialize storage path: {:?}", e)))?;
    std::os::unix::fs::symlink(storage_path, target_path).map_err(|e|
        Error::StorageError(format!("{:?}", e)))?;
    Ok(())
}*/

/// Run the compute request with the native backend.
///
/// Parameters:
/// - `binary_path`:  The path to a binary, hopefully compatible with the
///   platform of the current device.
/// - `args`: Arguments.
/// - `envs`: Environment variables.
/// - `karl_path`: Probably `~/.karl`.
/// - `base_path`: The service base path is usually at `~/.karl/<request_id>`.
///   The path to the computation root should be a directory within the service
///   base path, and should exist. The computation root contains the unpacked
///   and decompressed bytes of the compute request.
pub fn run(
    root_path: &Path,
    binary_path: PathBuf,
    args: Vec<String>,
    envs: Vec<String>,
) -> Result<(), Error> {
    // Directory layout
    // <request_id> = 123456
    // <client_id> = wyzecam
    //
    // .karl/*          # `karl_path`
    //   storage/
    //     wyzecam/     # `storage_path`
    //   123456/*       # request_id `base_path`
    //     234567/*     # computation root = `root_path`
    //       detect.py
    //       img.tmp
    //       storage/   # mounted .karl/storage/wyzecam (`storage_path`)
    //
    // * indicates the directory must exist ahead of time

    assert!(root_path.is_dir());

    /*
    // Create a symlink to persistent storage.
    if storage {
        #[cfg(target_os = "linux")]
        symlink_storage(karl_path, &root_path, client_id)?;
        #[cfg(target_os = "macos")]
        unimplemented!("storage is unimplemented on macos");
    }*/

    let now = Instant::now();
    let output = run_cmd(root_path, binary_path, envs, args);
    info!("=> execution: {} s", now.elapsed().as_secs_f32());

    // Return the requested results.
    debug!("\n{}", String::from_utf8_lossy(&output.stdout));
    debug!("{}", String::from_utf8_lossy(&output.stderr));
    Ok(())
}
