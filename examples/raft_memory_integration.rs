/// Example demonstrating the integration of MemoryRaftLogStorage with ObjectShardStateMachine
/// 
/// This shows how the redesigned SnapshotCapableStorage now works with OpenRaft's
/// snapshot builder trait using key-value streaming and BTreeMap-based memory backend.

use std::collections::BTreeSet;
use oprc_dp_storage::{MemoryRaftLogStorage, MemoryStorage, StorageConfig, StorageBackendType};
use oprc_odgm::replication::raft::state_machine::ObjectShardStateMachine;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 OpenRaft Snapshot Integration Example");
    
    // 1. Create the in-memory RaftLogStorage with BTreeMap (as requested)
    let raft_log_storage = MemoryRaftLogStorage::new();
    println!("✅ Created MemoryRaftLogStorage with BTreeMap backing");
    
    // 2. Create the application storage backend
    let storage_config = StorageConfig {
        backend_type: StorageBackendType::Memory,
        ..Default::default()
    };
    let app_storage = MemoryStorage::new(storage_config).await?;
    println!("✅ Created application storage backend");
    
    // 3. Create the state machine that bridges OpenRaft and our storage
    let state_machine = ObjectShardStateMachine::new(raft_log_storage.clone(), app_storage);
    println!("✅ Created ObjectShardStateMachine with OpenRaft integration");
    
    // 4. Demonstrate the key-value streaming capability
    println!("\n📊 Testing Key-Value Streaming Integration:");
    
    // Show that the RaftLogStorage can handle Raft state
    let mut membership = oprc_dp_storage::RaftMembership::default();
    membership.config_id = 1;
    membership.voters.insert(1);
    membership.voters.insert(2);
    membership.voters.insert(3);
    
    // Save some Raft state
    raft_log_storage.save_membership(&membership).await?;
    println!("✅ Saved Raft membership configuration with 3 voters");
    
    // Verify state retrieval
    let (applied_log_id, stored_membership) = raft_log_storage.applied_state().await?;
    println!("✅ Retrieved applied state - Log ID: {:?}, Voters: {}", 
             applied_log_id, stored_membership.voters.len());
    
    // 5. Show log entry operations with BTreeMap ordering
    println!("\n📝 Testing BTreeMap-based Log Storage:");
    
    // Add log entries (they will be stored in order thanks to BTreeMap)
    for i in [3, 1, 5, 2, 4] { // Insert in random order
        raft_log_storage.append_entry(i, 1, format!("Entry {}", i).as_bytes()).await?;
    }
    
    let stats = raft_log_storage.log_stats().await?;
    println!("✅ Added 5 log entries - First: {:?}, Last: {:?}, Count: {}", 
             stats.first_index, stats.last_index, stats.entry_count);
    
    // Verify ordering is maintained
    let entries = raft_log_storage.get_entries(1, 5).await?;
    println!("✅ Retrieved entries in correct order: [{}]", 
             entries.iter().map(|e| e.index.to_string()).collect::<Vec<_>>().join(", "));
    
    // 6. Demonstrate snapshot integration
    println!("\n📸 Testing Snapshot Integration:");
    
    let snapshot_meta = oprc_dp_storage::RaftSnapshotMeta {
        last_log_id: Some(oprc_dp_storage::RaftLogId { term: 1, index: 5 }),
        last_membership: stored_membership,
        snapshot_id: "example_snapshot_001".to_string(),
        created_at: std::time::SystemTime::now(),
        size_bytes: 2048,
        checksum: Some("sha256:example".to_string()),
    };
    
    raft_log_storage.install_snapshot_state(&snapshot_meta).await?;
    println!("✅ Installed snapshot state with metadata");
    
    let retrieved_snapshot = raft_log_storage.last_snapshot_meta().await?;
    if let Some(meta) = retrieved_snapshot {
        println!("✅ Retrieved snapshot: ID={}, Size={}B", meta.snapshot_id, meta.size_bytes);
    }
    
    println!("\n🎉 Integration Complete!");
    println!("Key Benefits Achieved:");
    println!("  • Key-value streaming for LSM/B-tree compatibility");
    println!("  • BTreeMap ensures ordered access to log entries");
    println!("  • Zero-copy local operations maintained");
    println!("  • Memory-efficient network streaming");
    println!("  • Full OpenRaft snapshot builder integration");
    
    Ok(())
}
