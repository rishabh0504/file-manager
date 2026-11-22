//! Operation history tracking for undo/audit purposes.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OperationType {
    Move,
    Copy,
    Delete,
    Rename,
    Extract,
    CreateFolder,
}

impl std::fmt::Display for OperationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OperationType::Move => write!(f, "Move"),
            OperationType::Copy => write!(f, "Copy"),
            OperationType::Delete => write!(f, "Delete"),
            OperationType::Rename => write!(f, "Rename"),
            OperationType::Extract => write!(f, "Extract"),
            OperationType::CreateFolder => write!(f, "Create Folder"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct OperationRecord {
    pub id: u64,
    pub op_type: OperationType,
    pub source: PathBuf,
    pub destination: Option<PathBuf>,
    pub success: bool,
    pub timestamp: u64,
    pub error_msg: Option<String>,
}

impl OperationRecord {
    pub fn new(op_type: OperationType, source: PathBuf) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            id: timestamp * 1000 + (std::time::SystemTime::now().elapsed().unwrap_or_default().subsec_millis() as u64),
            op_type,
            source,
            destination: None,
            success: false,
            timestamp,
            error_msg: None,
        }
    }

    pub fn with_destination(mut self, dest: PathBuf) -> Self {
        self.destination = Some(dest);
        self
    }

    pub fn with_success(mut self, success: bool) -> Self {
        self.success = success;
        self
    }

    pub fn with_error(mut self, error: String) -> Self {
        self.error_msg = Some(error);
        self
    }

    pub fn format_time(&self) -> String {
        // Format timestamp as HH:MM:SS
        let secs = self.timestamp % 86400;
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        let secs = secs % 60;
        format!("{:02}:{:02}:{:02}", hours, mins, secs)
    }

    pub fn summary(&self) -> String {
        let src = self.source.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?");
        
        let status = if self.success { "✅" } else { "❌" };
        
        match self.destination.as_ref() {
            Some(dst) => {
                let dst_name = dst.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("?");
                format!("{} {} {} → {}", status, self.op_type, src, dst_name)
            }
            None => format!("{} {} {}", status, self.op_type, src),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct OperationHistory {
    records: Vec<OperationRecord>,
    max_records: usize,
}

impl OperationHistory {
    pub fn new(max_records: usize) -> Self {
        Self {
            records: Vec::new(),
            max_records,
        }
    }

    pub fn add(&mut self, record: OperationRecord) {
        self.records.push(record);
        if self.records.len() > self.max_records {
            self.records.remove(0);
        }
    }

    pub fn records(&self) -> &[OperationRecord] {
        &self.records
    }

    pub fn records_mut(&mut self) -> &mut Vec<OperationRecord> {
        &mut self.records
    }

    pub fn clear(&mut self) {
        self.records.clear();
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn recent(&self, limit: usize) -> Vec<&OperationRecord> {
        self.records.iter().rev().take(limit).collect()
    }

    pub fn filter_by_type(&self, op_type: &OperationType) -> Vec<&OperationRecord> {
        self.records.iter().filter(|r| r.op_type == *op_type).collect()
    }
}
