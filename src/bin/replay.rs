#[macro_use]
extern crate log;

use std::fs;
use std::io::Write;
use std::time::Instant;

use wasmer::executor::{run, Run};

use karl::*;

/// Handle a compute request.
fn handle_compute(req: ComputeRequest) -> Result<ComputeResult, Error> {
	// Write the tar.gz bytes into a temporary file.
    info!("handling compute: (len {}) stdout={} stderr={} {:?}",
        req.package.len(), req.stdout, req.stderr, req.files);
    let now = Instant::now();
    let filename = "package.tar.gz";
    let mut f = fs::File::create(filename)?;
    f.write_all(&req.package[..])?;
    f.flush()?;
    debug!("=> write package.tar.gz: {} s", now.elapsed().as_secs_f32());

    // Replay the packaged computation.
    let now = Instant::now();
    let mut options = Run::new(std::path::Path::new(filename).to_path_buf());
    options.replay = true;
    let result = run(&mut options).expect("expected result");
    debug!("=> execution: {} s", now.elapsed().as_secs_f32());

    // Return the requested results.
    let now = Instant::now();
    let mut res = ComputeResult::new();
    if req.stdout {
        res.stdout = result.stdout;
    }
    if req.stderr {
        res.stderr = result.stderr;
    }
    for path in req.files {
        // TODO: use KARL_PATH environment variable
        let f = result.root.path().join(&path);
        match fs::File::open(&f) {
            Ok(mut file) => {
                res.files.insert(path, read_packet(&mut file, false)?);
            },
            Err(e) => warn!("error opening output file {:?}: {:?}", f, e),
        }
    }
    debug!("=> build result: {} s", now.elapsed().as_secs_f32());
    Ok(res)
}

/// Requests computation from the host.
fn generate_request() -> Result<ComputeRequest, Error> {
    info!("building compute request");
    let now = Instant::now();
    let request = ComputeRequestBuilder::new("python/python.wasm")
        .preopen_dirs(vec!["python/"])
        .args(vec!["python/run.py"])
        .build_root()?
        .add_dir("python/")?
        .finalize()?;
    // let mut f = File::open("package.tar.gz").expect("failed to open package.tar.gz");
    // let buffer = read_packet(&mut f, false).expect("failed to read package.tar.gz");
    // let request = ComputeRequest::new(buffer);
    info!("=> {} s", now.elapsed().as_secs_f32());

    info!("create compute request");
    let now = Instant::now();
    let filename = "python/tmp2.txt";
    let request = request
        .stdout()
        .stderr()
        .file(filename);
    info!("=> {} s", now.elapsed().as_secs_f32());
    Ok(request)
}

fn main() -> Result<(), Error> {
    env_logger::builder().format_timestamp(None).init();
    let request = generate_request()?;
    let result = handle_compute(request)?;
    println!("{:?}", result);
    Ok(())
}
