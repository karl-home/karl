use serde::{Serialize, Deserialize};

/// External import.
#[derive(Debug, Serialize, Deserialize)]
#[repr(C)]
pub enum Import {
    /// Wasm package manager
    Wapm {
        /// Package name
        name: String,
        /// Package version
        version: String,
    }
}
