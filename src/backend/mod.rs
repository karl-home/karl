//! Multiple backends for running the a Karl service.
//!
//! Specifies the format of the computation request it receives. It could
//! be WebAssembly, a generic binary, a Docker container...
pub mod binary;

/// Karl service or client backend.
///
/// Assumes services and clients within the same network configuration
/// run the same backend.
pub enum Backend {
	/// Linux executable.
	Binary,
}
