//! Information flow audit logs.
use std::collections::HashMap;
use std::time::Instant;
use std::path::{Path, PathBuf};
use crate::common::*;

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
    data_path: PathBuf,
    /// Currently active processes, updated as they start and finish
    active_processes: HashMap<ProcessToken, ProcessID>,
    /// Map from process IDs to their corresponding hook IDs, permanent
    process_hooks: HashMap<ProcessID, HookID>,
    /// Log entries indexed by process ID
    process_entries: HashMap<ProcessID, Vec<LogEntry>>,
    /// Log entries indexed by file path
    file_entries: HashMap<PathBuf, Vec<LogEntry>>,
    /// Origin of raw sensor files
    file_origins: HashMap<PathBuf, SensorID>,
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
    pub fn new(data_path: PathBuf) -> Self {
        Self {
            data_path,
            active_processes: HashMap::new(),
            process_hooks: HashMap::new(),
            process_entries: HashMap::new(),
            file_entries: HashMap::new(),
            file_origins: HashMap::new(),
        }
    }

    pub fn notify_start(
        &mut self,
        process_token: ProcessToken,
        process_id: ProcessID,
        hook_id: HookID,
    ) {
        assert!(self.active_processes.insert(process_token, process_id).is_none());
        assert!(self.process_entries.insert(process_id, Vec::new()).is_none());
        assert!(self.process_hooks.insert(process_id, hook_id).is_none());
    }

    pub fn notify_end(&mut self, process_token: ProcessToken) {
        assert!(self.active_processes.remove(&process_token).is_some());
    }

    pub fn push_sensor_data(&mut self, path: &Path, sensor_id: SensorID) {
        debug!("push_sensor_data {} {:?}", sensor_id, path);
        assert!(!self.file_origins.contains_key(path));
        self.file_origins.insert(path.to_path_buf(), sensor_id);
    }

    pub fn push_log(&mut self, process_id: ProcessID, entry_ty: LogEntryType) {
        let entry = LogEntry::new(process_id, entry_ty.clone());
        let mut next_path = match &entry_ty {
            LogEntryType::Put { path } => Some(Path::new(path)),
            LogEntryType::Get { path } => Some(Path::new(path)),
            LogEntryType::Delete { path } => Some(Path::new(path)),
            LogEntryType::Network { .. } => None,
        };
        while let Some(path) = next_path {
            self.file_entries
                .get_mut(path).unwrap()
                .push(entry.clone());
            next_path = path.parent();
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
