//! Multiple backends for running the a Karl service.
//!
//! Specifies the format of the computation request it receives. It could
//! be WebAssembly, a generic binary, a Docker container...
pub mod wasm;
