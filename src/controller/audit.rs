//! Information flow audit logs.
use std::collections::HashMap;
use std::time::Instant;
use std::path::{Path, PathBuf};
use crate::common::*;

#[derive(Debug, Clone)]
pub enum LogEntryType {
    Put { path: String },
    Get { path: String },
    Network { domain: String },
    State { sensor_id: String, key: String, value: String },
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
    /// Currently active processes, updated as they start and finish
    active_processes: HashMap<ProcessToken, ProcessID>,
    /// Map from process IDs to their corresponding hook IDs, permanent
    process_hooks: HashMap<ProcessID, HookID>,
    /// Log entries indexed by process ID
    process_entries: HashMap<ProcessID, Vec<LogEntry>>,
    /// Log entries indexed by file path
    file_entries: HashMap<PathBuf, Vec<LogEntry>>,
    /// Log entries indexed by sensor
    sensor_entries: HashMap<SensorID, Vec<LogEntry>>,
}

impl LogEntry {
    fn new(pid: ProcessID, ty: LogEntryType) -> Self {
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
            active_processes: HashMap::new(),
            process_hooks: HashMap::new(),
            process_entries: HashMap::new(),
            file_entries: HashMap::new(),
            sensor_entries: HashMap::new(),
        }
    }

    pub fn notify_start(
        &mut self,
        process_token: ProcessToken,
        process_id: ProcessID,
        hook_id: HookID,
    ) {
        info!("PID {} start from hook {}", process_id, hook_id);
        assert!(self.active_processes.insert(process_token, process_id).is_none());
        assert!(self.process_entries.insert(process_id, Vec::new()).is_none());
        assert!(self.process_hooks.insert(process_id, hook_id).is_none());
    }

    pub fn notify_end(&mut self, process_token: ProcessToken) {
        let process_id = self.active_processes.remove(&process_token).unwrap();
        info!("PID {} finish", process_id);
    }

    /// Returns false if no process ID for the token.
    pub fn push(&mut self, token: &ProcessToken, entry_ty: LogEntryType) {
        let process_id = *self.active_processes.get(token).expect("invalid token");
        let entry = LogEntry::new(process_id, entry_ty);
        let mut next_path = match &entry.ty {
            LogEntryType::Put { path } => {
                info!("PID {} put {}", process_id, path);
                Some(Path::new(path))
            },
            LogEntryType::Get { path } => {
                info!("PID {} get {}", process_id, path);
                Some(Path::new(path))
            },
            LogEntryType::Network { .. } => None,
            LogEntryType::State { .. } => None,
        };
        match &entry.ty {
            LogEntryType::Network { domain } => {
                info!("PID {} network {}", process_id, domain);
            },
            LogEntryType::State { sensor_id, key, value } => {
                info!("PID {} state {} {} => {}", process_id, &sensor_id, &key, &value);
                self.sensor_entries
                    .entry(sensor_id.to_string())
                    .or_insert(vec![])
                    .push(entry.clone());
            },
            _ => {},
        }
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

    pub fn audit_process(&self, process_id: ProcessID) -> Option<Vec<String>> {
        self.process_entries
            .get(&process_id)
            .map(|entries| entries.iter().map(|entry| format!("{:?}", entry)).collect())
    }

    pub fn audit_file(&self, path: &Path) -> Option<Vec<String>> {
        self.file_entries
            .get(path)
            .map(|entries| entries.iter().map(|entry| format!("{:?}", entry)).collect())
    }

    pub fn audit_sensor(&self, sensor_id: &SensorID) -> Option<Vec<String>> {
        self.sensor_entries
            .get(sensor_id)
            .map(|entries| entries.iter().map(|entry| format!("{:?}", entry)).collect())
    }
}

#[cfg(test)]
mod test {
    // TODO
}
