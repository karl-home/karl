#[macro_use]
extern crate log;

mod error;
mod import;
mod request;

pub type HeaderType = u32;
pub const HT_RAW_BYTES: HeaderType = 0;
pub const HT_PING_REQUEST: HeaderType = 1;
pub const HT_PING_RESULT: HeaderType = 2;
pub const HT_COMPUTE_REQUEST: HeaderType = 3;
pub const HT_COMPUTE_RESULT: HeaderType = 4;
pub const HT_HOST_REQUEST: HeaderType = 5;
pub const HT_HOST_RESULT: HeaderType = 6;

pub use error::Error;
pub use import::Import;
pub use request::{
	ComputeRequest, ComputeRequestBuilder, ComputeResult,
	PingRequest, PingResult, HostRequest, HostResult,
};
pub use wasmer::executor::PkgConfig;
