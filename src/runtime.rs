//! Code for executing a process inside a host.
//!
//! Runtime for arbitrary Linux executables that mounts the appropriate
//! directories, configures environment variables, and otherwise manages
//! the sandbox for offloading compute requests.
use std::env;
use std::fs;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Instant;

#[cfg(target_os = "linux")]
use sys_mount::{SupportedFilesystems, Mount, MountFlags, Unmount, UnmountFlags};
use crate::protos::ComputeResult;
use crate::common::Error;

fn run_cmd(bin: PathBuf, envs: Vec<String>, args: Vec<String>) -> Output {
    debug!("bin: {:?}", bin);
    debug!("envs: {:?}", envs);
    debug!("args: {:?}", args);
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

/// Copies a directory from the old (mapped) directory to the new (root)
/// directory. The directory path is a relative path.
#[cfg(target_os = "macos")]
fn copy(path: &Path, old_dir: &Path, new_dir: &Path) {
    assert!(path.is_relative());
    let abs_path = old_dir.join(path);
    assert!(abs_path.is_dir());
    for f in fs::read_dir(&abs_path).unwrap() {
        let f = f.unwrap();
        let ext_path = path.join(f.file_name());
        let old_path = old_dir.join(&ext_path);
        let new_path = new_dir.join(&ext_path);
        if old_path.is_dir() {
            fs::create_dir_all(new_path).unwrap();
            copy(&ext_path, old_dir, new_dir);
        } else {
            fs::copy(old_path, new_path).unwrap();
        }
    }
}

/// Copy an old path to the new path.
#[cfg(target_os = "macos")]
fn copy_mapped_dirs(mapped_dirs: Vec<String>) -> Result<(), Error> {
    for mapped_dir in mapped_dirs {
        let mut mapped_dir = mapped_dir.split(":");
        let new_dir = Path::new(mapped_dir.next().unwrap());
        let old_dir = Path::new(mapped_dir.next().unwrap());
        assert!(mapped_dir.next().is_none());
        copy(Path::new("."), old_dir, new_dir);
    }
    Ok(())
}

/// Mounts an overlay fs at the `root_path`. All mapped directories are from
/// a read-only directory containing dependency files to the current working
/// directory. The `work_path` is needed in overlay fs to prepare files before
/// they are switched to the overlay destination in an atomic action.
///
/// Parameters:
/// - mapped_dirs - List of directories to map to the current directory.
/// - root_path - The initial filesystem provided by the client, and the
///   eventual working directory.
/// - work_path - Needed for the overlay fs.
///
/// Returns:
/// An object representing the mounted filesystem. Once the reference to the
/// object is dropped, the directory is also unmounted.
/// If no mount is needed i.e. no mapped dirs, returns None.
#[cfg(target_os = "linux")]
fn mount_imports(
    mapped_dirs: Vec<String>,
    root_path: &Path,
    work_path: &Path,
) -> Option<Mount> {
    // no mapped dirs
    if mapped_dirs.is_empty() {
        return None;
    }

    // mount imports as mapped dirs
    let fstype = "overlay";
    assert!(SupportedFilesystems::new().unwrap().is_supported(fstype));
    let lowerdir = {
        let mapped_dirs = mapped_dirs
            .iter()
            .map(|mapped_dir| {
                let mut mapped_dir = mapped_dir.split(":");
                let new_dir = mapped_dir.next().unwrap();
                let old_dir = mapped_dir.next().unwrap();
                assert_eq!(new_dir, ".");
                assert!(Path::new(old_dir).is_dir());
                old_dir
            })
            .collect::<Vec<_>>();
        assert!(!mapped_dirs.is_empty());
        mapped_dirs.join(":")
    };
    let options = format!(
        "lowerdir={},upperdir={},workdir={}",
        lowerdir,
        root_path.to_str().unwrap(),
        work_path.to_str().unwrap(),
    );
    debug!("mounting to {:?} fstype={:?} options={:?}", root_path, fstype, options);
    Some(Mount::new(
        "dummy",  // dummy
        root_path,
        fstype,
        MountFlags::empty(),
        Some(&options),
    ).unwrap())
}

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
}

/// Run the compute request with the native backend.
///
/// Parameters:
/// - `binary_path`:  The path to a binary, hopefully compatible with the
///   platform of the current device.
/// - `mapped_dirs`: The implementation _copies_ the files inside the original
///   directory into the `root_path`. If there is already a file in the
///   `root_path` with the same relative filename, the file is not copied.
///   Earlier mapped directories have priority over later ones. Ideally, this
///   is mapped in the syscall layer.
/// - `args`: Arguments.
/// - `envs`: Environment variables.
/// - `karl_path`: Probably `~/.karl`.
/// - `base_path`: The service base path is usually at `~/.karl/<request_id>`.
///   The path to the computation root should be a directory within the service
///   base path, and should exist. The computation root contains the unpacked
///   and decompressed bytes of the compute request.
/// - `client_id`: Client ID, used to determine persistent storage path.
/// - `storage`: Whether to use persistent storage. Mounts the persistent
///   storage path at `<base_path>`
/// - `res_stdout`: Whether to include stdout in the result.
/// - `res_stderr`: Whether to include stderr in the result.
/// - `res_files`: Files to include in the result, if they exist.
pub fn run(
    binary_path: PathBuf,
    mapped_dirs: Vec<String>,
    args: Vec<String>,
    envs: Vec<String>,
    karl_path: &Path,
    base_path: &Path,
    client_id: &str,
    storage: bool,
    res_stdout: bool,
    res_stderr: bool,
    res_files: HashSet<String>,
) -> Result<ComputeResult, Error> {
    // Directory layout
    // <request_id> = 123456
    // <client_id> = wyzecam
    //
    // .karl/*          # `karl_path`
    //   storage/
    //     wyzecam/     # `storage_path`
    //   123456/*       # request_id `base_path`
    //     work/        # `work_path`
    //     root/*       # computation root = `root_path`
    //       detect.py
    //       img.tmp
    //       storage/   # mounted .karl/storage/wyzecam (`storage_path`)
    //
    // * indicates the directory must exist ahead of time

    let now = Instant::now();
    let previous_dir = fs::canonicalize(".").unwrap();
    assert!(base_path.is_dir());
    let root_path = base_path.join("root");
    assert!(root_path.is_dir());

    // Map directories using an overlay fs if possible, but otherwise
    // copy all the files into the root path. This can be very slow!
    #[cfg(target_os = "macos")]
    {
        env::set_current_dir(&root_path).unwrap();
        copy_mapped_dirs(mapped_dirs)?;
        info!("=> mapped_dirs: {} s", now.elapsed().as_secs_f32());
    }
    #[cfg(target_os = "linux")]
    let mount_result = {
        let work_path = base_path.join("work");
        fs::create_dir_all(&work_path).unwrap();
        let mount_result = mount_imports(mapped_dirs, &root_path, &work_path);
        env::set_current_dir(&root_path).unwrap();
        info!("=> mounted fs: {} s", now.elapsed().as_secs_f32());
        mount_result
    };
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        unimplemented!();
    }

    // Create a symlink to persistent storage.
    if storage {
        #[cfg(target_os = "linux")]
        symlink_storage(karl_path, &root_path, client_id)?;
        #[cfg(target_os = "macos")]
        unimplemented!("storage is unimplemented on macos");
    }

    let now = Instant::now();
    let output = run_cmd(binary_path, envs, args);
    info!("=> execution: {} s", now.elapsed().as_secs_f32());

    // Return the requested results.
    warn!("{}", String::from_utf8_lossy(&output.stdout));
    warn!("{}", String::from_utf8_lossy(&output.stderr));
    let now = Instant::now();
    let mut res = ComputeResult::default();
    if res_stdout {
        res.set_stdout(output.stdout);
    }
    if res_stderr {
        res.set_stderr(output.stderr);
    }
    for path in res_files {
        let f = root_path.join(&path);
        match fs::read(&f) {
            Ok(bytes) => { res.mut_files().insert(path, bytes); },
            Err(e) => warn!("error reading output file {:?}: {:?}", f, e),
        }
    }
    info!("=> build result: {} s", now.elapsed().as_secs_f32());
    env::set_current_dir(&previous_dir).unwrap();
    #[cfg(target_os = "linux")]
    if let Some(mount_result) = mount_result {
        // Note that a filesystem cannot be unmounted when it is 'busy' - for
        // example, when there are open files on it, or when some process has
        // its working directory there, or when a swap file on it is in use.
        if let Err(e) = mount_result.unmount(UnmountFlags::DETACH) {
            error!("error unmounting: {:?}", e);
        }
    }
    Ok(res)
}

#[cfg(test)]
mod test {
    //! NOTE: Tests cannot be run concurrently with any other test.
    //! Use `cargo test -- --ignored` or `cargo test -- --test-threads=1`.
    use super::*;
    use serial_test::serial;

    /// Temporary base path 'data/<name>' corresponding to '~/.karl/<ID>'.
    /// Base path is initialized with root path 'root' containing input
    /// filesystem (the audio file).
    /// Workdir path 'work' is also created on Linux for overlayfs.
    fn init_base_path() -> PathBuf {
        let base_path = Path::new("data/tmp-test");
        if base_path.exists() {
            fs::remove_dir_all(&base_path).unwrap();
        }
        fs::create_dir_all(&base_path).unwrap();
        let base_path = fs::canonicalize(&base_path).unwrap();
        fs::create_dir(&base_path.join("root")).expect("create root path");
        #[cfg(target_os = "linux")]
        fs::create_dir(&base_path.join("work")).expect("create work path");
        base_path
    }

    /// Test karl path is 'data'.
    fn init_karl_path() -> PathBuf {
        fs::canonicalize(&Path::new("data")).unwrap()
    }

    /// Runs STT example contained in a temporary base path in this directory.
    /// Data path 'data/stt' should be initialized with 'scripts/setup_stt.sh'.
    ///
    /// Inputs to package config are as follows. Binary path is an absolute
    /// path to the binary in the root path, which will be mounted.
    /// Data path is mapped to the root.
    /// Arguments assume the process will run in the root path and are relative
    /// paths. PYTHONPATH is also set with relative paths.
    ///
    /// Check stdout and stderr.
    #[test]
    #[ignore]
    #[serial]
    fn run_stt_python() {
        let base_path = init_base_path();
        fs::copy(
            "data/stt/audio/2830-3980-0043.wav",
            "data/tmp-test/root/2830-3980-0043.wav",
        ).unwrap();
        let root_path = base_path.join("root");
        let binary_path = fs::canonicalize(root_path).unwrap().join("bin/python");
        let data_path = {
            let path = Path::new("data/stt");
            assert!(path.exists(), "run scripts/setup_stt.sh");
            let path = fs::canonicalize(path).unwrap();
            path.into_os_string().into_string().unwrap()
        };
        let mapped_dirs = vec![format!(".:{}", data_path)];
        let args = vec![
            "client.py".to_string(),
            "--model".to_string(),
            "models.pbmm".to_string(),
            "--scorer".to_string(),
            "models.scorer".to_string(),
            "--audio".to_string(),
            "2830-3980-0043.wav".to_string(),
        ];
        let envs = vec![
            "PYTHONPATH=\
            lib/python3.6/:\
            lib/python3.6/lib-dynload:\
            lib/python3.6/site-packages".to_string()
        ];
        let karl_path = init_karl_path();
        let client_id = "test_stt_python";
        let storage = false;
        let res_stdout = true;
        let res_stderr = true;
        let res_files = HashSet::new();
        let res = run(
            binary_path,
            mapped_dirs,
            args,
            envs,
            &karl_path,
            &base_path,
            client_id,
            storage,
            res_stdout,
            res_stderr,
            res_files,
        );
        fs::remove_dir_all(&base_path).unwrap();
        match res {
            Ok(res) => {
                let stdout = String::from_utf8_lossy(&res.stdout);
                let stderr = String::from_utf8_lossy(&res.stderr);
                assert_eq!("experience proves this", stdout.trim());
                assert!(!stderr.is_empty(), "stderr output requested");
                assert!(res.files.is_empty(), "not expecting any files");
            },
            Err(e) => assert!(false, format!("failed run: {:?}", e)),
        }
    }

    /// Runs STT example contained in a temporary base path in this directory.
    /// Data path 'data/stt_node' should be initialized with
    /// 'scripts/setup_stt_node.sh'.
    ///
    /// Inputs to package config are as follows. Binary path is an absolute
    /// path to the binary in the root path, which will be mounted.
    /// Data path is mapped to the root.
    /// Arguments assume the process will run in the root path and are relative
    /// paths.
    ///
    /// Check stdout and stderr.
    #[test]
    #[ignore]
    #[serial]
    fn run_stt_node() {
        let base_path = init_base_path();
        fs::copy(
            "data/stt/audio/2830-3980-0043.wav",
            "data/tmp-test/root/2830-3980-0043.wav",
        ).unwrap();
        let root_path = base_path.join("root");
        let binary_path = fs::canonicalize(root_path).unwrap().join("bin/node");
        let data_path = {
            let path = Path::new("data/stt_node");
            assert!(path.exists(), "run scripts/setup_stt_node.sh");
            let path = fs::canonicalize(path).unwrap();
            path.into_os_string().into_string().unwrap()
        };
        let mapped_dirs = vec![format!(".:{}", data_path)];
        let args = vec![
            "main.js".to_string(),
            "weather.wav".to_string(),
            "models.pbmm".to_string(),
            "models.scorer".to_string(),
        ];
        let envs = vec![];
        let karl_path = init_karl_path();
        let client_id = "test_stt_node";
        let storage = false;
        let res_stdout = true;
        let res_stderr = true;
        let res_files = HashSet::new();
        let res = run(
            binary_path,
            mapped_dirs,
            args,
            envs,
            &karl_path,
            &base_path,
            client_id,
            storage,
            res_stdout,
            res_stderr,
            res_files,
        );
        fs::remove_dir_all(&base_path).unwrap();
        match res {
            Ok(res) => {
                let stdout = String::from_utf8_lossy(&res.stdout);
                let stderr = String::from_utf8_lossy(&res.stderr);
                assert_eq!("what is the weather to day in san francisco california", stdout.trim());
                assert!(!stderr.is_empty(), "stderr output requested");
                assert!(res.files.is_empty(), "not expecting any files");
            },
            Err(e) => assert!(false, format!("failed run: {:?}", e)),
        }
    }
}
