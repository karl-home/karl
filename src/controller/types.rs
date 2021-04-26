use std::collections::HashSet;
use std::net::{SocketAddr, IpAddr};
use std::time::Instant;
use serde::{Serialize, ser::{Serializer, SerializeStruct}};
use crate::common::ProcessToken;

/// Self-assigned string ID uniquely identifying a host.
pub type HostID = String;
/// Self-assigned string ID uniquely identifying a sensor.
pub type SensorID = String;
/// ID uniquely identifying a host, assigned by the controller
/// in incrementing order starting at 0.
pub type ProcessID = u32;
/// String ID uniquely identifying a hook, assigned by the controller
/// on registration based on the global hook ID, and verified by a user.
pub type HookID = String;

/// Host status and information.
#[derive(Debug, Clone)]
pub struct Host {
    /// Whether the user has confirmed this host.
    pub(crate) confirmed: bool,
    /// Index, used internally.
    pub(crate) index: usize,
    /// Host ID.
    pub id: HostID,
    /// Host address.
    pub addr: SocketAddr,
    /// Metadata.
    pub md: HostMetadata,
}

#[derive(Debug, Clone)]
pub struct HostMetadata {
    /// All active requests.
    pub active_requests: HashSet<ProcessToken>,
    /// Time of last heartbeat, notify start, or notify end.
    pub last_msg: Instant,
    /// Total number of requests handled.
    pub total: usize,
}

/// Client status and information.
#[derive(Serialize, Debug, Clone)]
pub struct Client {
    /// Whether the user has confirmed this client.
    pub confirmed: bool,
    /// The self-given lowercase alphanumeric and underscore ID of the client,
    /// with _1, _2, etc. appended when duplicates are registered, like handling
    /// duplicates in the filesystem.
    pub id: SensorID,
    /// State keys.
    pub keys: Vec<String>,
    /// Output tags.
    pub tags: Vec<String>,
    /// IP address for proxy requests.
    pub addr: IpAddr,
}

/// Request information.
#[derive(Debug, Clone)]
pub struct Request {
    /// Description of request.
    pub description: String,
    /// Request start time.
    pub start: Instant,
    /// Request end time.
    pub end: Option<Instant>,
}

impl Host {
    pub fn is_confirmed(&self) -> bool {
        self.confirmed
    }
}

impl Serialize for Host {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Host", 9)?;
        state.serialize_field("confirmed", &self.is_confirmed())?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("addr", &self.addr)?;
        state.serialize_field("active_requests", &self.md.active_requests)?;
        state.serialize_field("last_msg", &self.md.last_msg.elapsed().as_secs_f32())?;
        state.serialize_field("total", &self.md.total)?;
        state.end()
    }
}

impl Default for HostMetadata {
    fn default() -> Self {
        Self {
            active_requests: HashSet::new(),
            last_msg: Instant::now(),
            total: 0,
        }
    }
}

impl Serialize for Request {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let time = if let Some(end) = self.end {
            (end - self.start).as_secs_f32()
        } else {
            self.start.elapsed().as_secs_f32()
        };
        let mut state = serializer.serialize_struct("Request", 2)?;
        state.serialize_field("description", &self.description)?;
        state.serialize_field("time", &time)?;
        state.end()
    }
}

impl Default for Request {
    fn default() -> Self {
        Request::new("".to_string())
    }
}

impl Request {
    pub fn new(description: String) -> Self {
        Request {
            description,
            start: Instant::now(),
            end: None,
        }
    }
}
