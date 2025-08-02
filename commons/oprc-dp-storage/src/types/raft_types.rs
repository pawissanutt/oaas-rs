use crate::StorageValue;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

/// Raft log entry with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaftLogEntry {
    pub index: u64,
    pub term: u64,
    pub entry_type: RaftLogEntryType,
    pub data: StorageValue, // Changed from Vec<u8> to StorageValue
    pub timestamp: SystemTime,
    pub checksum: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RaftLogEntryType {
    Normal,        // Regular application command
    Configuration, // Membership change
    Snapshot,      // Snapshot marker
    NoOp,          // No-operation (for leader election)
}

#[derive(Debug, Clone)]
pub struct RaftLogStats {
    pub first_index: Option<u64>,
    pub last_index: Option<u64>,
    pub entry_count: u64,
    pub total_size_bytes: u64,
    pub compacted_entries: u64,
    pub write_throughput_ops_per_sec: f64,
    pub avg_entry_size_bytes: f64,
}

/// Raft Log ID - uniquely identifies a log entry by term and index
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaftLogId {
    pub term: u64,
    pub index: u64,
}

/// Raft Hard State - persistent state that must survive restarts
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaftHardState {
    /// Current term number
    pub current_term: u64,
    /// Candidate voted for in current term (None if no vote cast)
    pub voted_for: Option<u64>,
}

/// Raft Membership - cluster configuration at a point in time
/// This is a simplified version - in practice you might want to use OpenRaft's types directly
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaftMembership {
    /// Configuration index - increases with each membership change
    pub config_id: u64,
    /// Set of voting nodes in the cluster
    pub voters: std::collections::BTreeSet<u64>,
    /// Set of learner nodes (non-voting)
    pub learners: std::collections::BTreeSet<u64>,
}

/// Snapshot metadata for Raft integration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaftSnapshotMeta {
    /// Last log entry included in this snapshot
    pub last_log_id: Option<RaftLogId>,
    /// Membership configuration at the time of snapshot
    pub last_membership: RaftMembership,
    /// Unique identifier for this snapshot
    pub snapshot_id: String,
    /// Timestamp when snapshot was created
    pub created_at: SystemTime,
    /// Size of the snapshot data in bytes
    pub size_bytes: u64,
    /// Checksum of snapshot data (optional integrity check)
    pub checksum: Option<String>,
}

impl Default for RaftHardState {
    fn default() -> Self {
        Self {
            current_term: 0,
            voted_for: None,
        }
    }
}

impl Default for RaftMembership {
    fn default() -> Self {
        Self {
            config_id: 0,
            voters: std::collections::BTreeSet::new(),
            learners: std::collections::BTreeSet::new(),
        }
    }
}

impl Default for RaftLogEntryType {
    fn default() -> Self {
        Self::Normal
    }
}

impl RaftLogEntry {
    /// Create a new normal log entry with StorageValue
    pub fn new_normal(index: u64, term: u64, data: StorageValue) -> Self {
        Self {
            index,
            term,
            entry_type: RaftLogEntryType::Normal,
            data,
            timestamp: SystemTime::now(),
            checksum: None,
        }
    }

    /// Create a new configuration change entry
    pub fn new_configuration(index: u64, term: u64, data: StorageValue) -> Self {
        Self {
            index,
            term,
            entry_type: RaftLogEntryType::Configuration,
            data,
            timestamp: SystemTime::now(),
            checksum: None,
        }
    }

    /// Get the size of this entry in bytes
    pub fn size_bytes(&self) -> usize {
        self.data.len() + 
        std::mem::size_of::<u64>() * 2 + // index + term
        std::mem::size_of::<RaftLogEntryType>() +
        std::mem::size_of::<SystemTime>() +
        std::mem::size_of::<Option<u64>>()
    }
}

impl RaftLogId {
    pub fn new(term: u64, index: u64) -> Self {
        Self { term, index }
    }

    /// Check if this log ID is newer than another
    pub fn is_newer_than(&self, other: &RaftLogId) -> bool {
        self.term > other.term || (self.term == other.term && self.index > other.index)
    }
}

impl RaftMembership {
    pub fn new(config_id: u64, voters: std::collections::BTreeSet<u64>, learners: std::collections::BTreeSet<u64>) -> Self {
        Self {
            config_id,
            voters,
            learners,
        }
    }

    /// Check if a node is a voter
    pub fn is_voter(&self, node_id: u64) -> bool {
        self.voters.contains(&node_id)
    }

    /// Check if a node is a learner
    pub fn is_learner(&self, node_id: u64) -> bool {
        self.learners.contains(&node_id)
    }

    /// Get all member node IDs (voters + learners)
    pub fn all_members(&self) -> std::collections::BTreeSet<u64> {
        self.voters.union(&self.learners).cloned().collect()
    }

    /// Get the total number of members
    pub fn member_count(&self) -> usize {
        self.voters.len() + self.learners.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raft_log_entry_with_storage_value() {
        let data = StorageValue::from("test_entry_data");
        let entry = RaftLogEntry::new_normal(1, 1, data.clone());

        assert_eq!(entry.index, 1);
        assert_eq!(entry.term, 1);
        assert_eq!(entry.data, data);
        assert!(matches!(entry.entry_type, RaftLogEntryType::Normal));
    }

    #[test]
    fn test_raft_log_id_comparison() {
        let log1 = RaftLogId::new(1, 100);
        let log2 = RaftLogId::new(1, 101);
        let log3 = RaftLogId::new(2, 50);

        assert!(log2.is_newer_than(&log1));
        assert!(log3.is_newer_than(&log1));
        assert!(log3.is_newer_than(&log2));
        assert!(!log1.is_newer_than(&log2));
    }

    #[test]
    fn test_raft_membership() {
        let mut voters = std::collections::BTreeSet::new();
        voters.insert(1);
        voters.insert(2);
        voters.insert(3);

        let mut learners = std::collections::BTreeSet::new();
        learners.insert(4);
        learners.insert(5);

        let membership = RaftMembership::new(1, voters, learners);

        assert!(membership.is_voter(1));
        assert!(membership.is_voter(2));
        assert!(membership.is_voter(3));
        assert!(!membership.is_voter(4));
        assert!(!membership.is_voter(5));

        assert!(membership.is_learner(4));
        assert!(membership.is_learner(5));
        assert!(!membership.is_learner(1));

        assert_eq!(membership.member_count(), 5);
        assert_eq!(membership.all_members().len(), 5);
    }
}
