use std::env;
use std::fs;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Instant;

use sys_mount::{SupportedFilesystems, Mount, MountFlags};
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

/// Copies a directory from the old (mapped) directory to the new (root)
/// directory. The directory path is a relative path.
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
#[allow(dead_code)]
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
fn mount(
    mapped_dirs: Vec<String>,
    root_path: &Path,
    work_path: &Path,
) -> Mount {
    let fstype = "overlay";
    assert!(SupportedFilesystems::new().unwrap().is_supported(fstype));
    let lowerdir = mapped_dirs
        .iter()
        .map(|mapped_dir| {
            let mut mapped_dir = mapped_dir.split(":");
            let new_dir = mapped_dir.next().unwrap();
            let old_dir = mapped_dir.next().unwrap();
            assert_eq!(new_dir, ".");
            assert!(Path::new(old_dir).is_dir());
            old_dir
        })
        .collect::<Vec<_>>()
        .join(":");
    let options = format!(
        "lowerdir={},upperdir={},workdir={}",
        lowerdir,
        root_path.to_str().unwrap(),
        work_path.to_str().unwrap(),
    );
    warn!("mounting to {:?} fstype={:?} options={:?}", root_path, fstype, options);
    Mount::new(
        "dummy",  // dummy
        root_path,
        fstype,
        MountFlags::empty(),
        Some(&options),
    ).unwrap()
}

/// Run the compute request with the native backend.
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
/// - `base_path`: The service base path is usually at `~/.karl/<id>`. The
///   path to the computation root should be a directory within the service
///   base path, and should exist. The computation root contains the unpacked
///   and decompressed bytes of the compute request.
/// - `res_stdout`: Whether to include stdout in the result.
/// - `res_stderr`: Whether to include stderr in the result.
/// - `res_files`: Files to include in the result, if they exist.
pub fn run(
    config: PkgConfig,
    base_path: &Path,
    res_stdout: bool,
    res_stderr: bool,
    res_files: HashSet<String>,
) -> Result<ComputeResult, Error> {
    let now = Instant::now();
    assert!(base_path.is_dir());
    let root_path = base_path.join("root");
    assert!(root_path.is_dir());
    let work_path = base_path.join("work");
    fs::create_dir_all(&work_path).unwrap();
    let mount_result = mount(config.mapped_dirs, &root_path, &work_path);
    env::set_current_dir(&root_path).unwrap();
    info!("=> mapped_dirs: {} s", now.elapsed().as_secs_f32());

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
    drop(mount_result);
    Ok(res)
}
