//! Multiple backends for running the a Karl service.
//!
//! Specifies the format of the computation request it receives. It could
//! be WebAssembly, a generic binary, a Docker container...
#[macro_use]
extern crate log;
#[cfg(target_os = "linux")]
extern crate sys_mount;
#[cfg(test)]
extern crate serial_test;

pub mod wasm;
pub mod binary;

/// Karl service or client backend.
///
/// Assumes services and clients within the same network configuration
/// run the same backend. Not currently developing the Wasm backend since
/// it has been nearly impossible to compile any useful applications to
/// WebAssembly.
pub enum Backend {
	/// Wasm executable.
	Wasm,
	/// MacOS executable.
	Binary,
}
