mod error;
mod builder;
mod token;
mod types;
mod hook;

pub use error::Error;
pub use builder::TarBuilder;
pub use hook::Hook;
pub use token::*;
pub use types::*;

/// Frequency at which the host must send messages to the controller, in seconds.
pub const HEARTBEAT_INTERVAL: u64 = 10;
/// Where hooks are stored in the local controller.
pub const HOOK_STORE_PATH: &str = "hooks";