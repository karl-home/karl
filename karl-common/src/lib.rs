mod error;
mod builder;
mod token;
mod module;

pub use error::Error;
pub use builder::TarBuilder;
pub use module::Module;
pub use token::*;

/// Frequency at which the host must send messages to the controller, in seconds.
pub const HEARTBEAT_INTERVAL: u64 = 10;

pub type Tag = String;
pub type HostID = String;
pub type SensorID = String;
pub type ProcessID = u32;
pub type GlobalModuleID = String;
pub type ModuleID = String;
