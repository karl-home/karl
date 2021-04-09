//! Information flow audit logs.
use std::collections::HashMap;
use std::time::Instant;
use std::path::{Path, PathBuf};
use crate::controller::types::*;

#[derive(Debug, Clone)]
pub enum LogEntryType {
    Put { path: String },
    Get { path: String },
    Delete { path: String },
    Network { domain: String },
}

/// Log entry.
#[derive(Debug, Clone)]
pub struct LogEntry {
    timestamp: Instant,
    pid: ProcessID,
    ty: LogEntryType,
}

/// Audit log.
pub struct AuditLog {
    order: Vec<ProcessID>,
    process_hooks: HashMap<ProcessID, HookID>,
    process_entries: HashMap<ProcessID, Vec<LogEntry>>,
    /// Raw sensor files only.
    file_creators: HashMap<PathBuf, SensorID>,
    file_entries: HashMap<PathBuf, Vec<LogEntry>>,
}

impl LogEntry {
    pub fn new(pid: ProcessID, ty: LogEntryType) -> Self {
        Self {
            timestamp: Instant::now(),
            pid,
            ty,
        }
    }
}

impl AuditLog {
    pub fn new() -> Self {
        Self {
            order: Vec::new(),
            process_hooks: HashMap::new(),
            process_entries: HashMap::new(),
            file_creators: HashMap::new(),
            file_entries: HashMap::new(),
        }
    }

    pub fn start_process(&mut self, process_id: ProcessID, hook_id: HookID) {
        assert!(!self.process_entries.contains_key(&process_id));
        assert!(!self.process_hooks.contains_key(&process_id));
        self.order.push(process_id);
        self.process_entries.insert(process_id, Vec::new());
        self.process_hooks.insert(process_id, hook_id);
    }

    pub fn create_file(&mut self, path: &Path, sensor_id: SensorID) {
        assert!(!self.file_creators.contains_key(path));
        self.file_creators.insert(path.to_path_buf(), sensor_id);
    }

    pub fn push_log(&mut self, process_id: ProcessID, entry_ty: LogEntryType) {
        let entry = LogEntry::new(process_id, entry_ty.clone());
        let path = match &entry_ty {
            LogEntryType::Put { path } => Some(path),
            LogEntryType::Get { path } => Some(path),
            LogEntryType::Delete { path } => Some(path),
            LogEntryType::Network { .. } => None,
        };
        if let Some(path) = path {
            // TODO: and all the parent directories
            self.file_entries
                .get_mut(Path::new(path)).unwrap()
                .push(entry.clone());
        }
        self.process_entries
            .get_mut(&process_id).unwrap()
            .push(entry);
    }

    pub fn audit_process(&self, process_id: ProcessID) -> Vec<LogEntry> {
        unimplemented!()
    }

    pub fn audit_file(&self, path: &Path) -> Vec<LogEntry> {
        unimplemented!()
    }
}

#[cfg(test)]
mod test {
    // TODO
}
