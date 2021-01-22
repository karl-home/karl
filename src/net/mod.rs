//! Network configuration for Karl services and their clients.
//!
//! Once a computer is running a Karl service and listening on a port, the
//! computer can register itself on DNS-SD using this package. This package
//! is mostly a thin wrapper around a simpler DNS-SD package.
//!
//! Clients also use this package to discover available Karl services via
//! DNS-SD. Currently, each client runs its own controller, which is aware of
//! all available Karl services. Eventually, the network configuration may
//! include a central controller where clients request available services.
#[cfg(feature = "dnssd")]
mod register;
mod executor;

pub use executor::*;
#[cfg(feature = "dnssd")]
pub use register::register;
