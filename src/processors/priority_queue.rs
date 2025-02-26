use crate::utils::events::Event;
use std::cmp::Ordering;
use crate::structs::requests::{CommitRequest, PrevoteRequest, ProposeRequest};

/// Assign priorities: **Lower value means higher priority**
#[derive(Debug)]
pub enum RBCMessage {
    RoundFinalized(u64),    // ✅ Priority 0 (Highest)
    Commit(CommitRequest),  // ✅ Priority 1
    Prevote(PrevoteRequest), // ✅ Priority 2
    Proposal(ProposeRequest), // ✅ Priority 3
}

// ✅ Manually Implement `Eq` Based on Priority (Ignore Inner Data)
impl PartialEq for RBCMessage {
    fn eq(&self, other: &Self) -> bool {
        self.priority() == other.priority()
    }
}

impl Eq for RBCMessage {}

impl Ord for RBCMessage {
    fn cmp(&self, other: &Self) -> Ordering {
        other.priority().cmp(&self.priority()) // **Reverse ordering for priority queue**
    }
}

impl PartialOrd for RBCMessage {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// ✅ Function to Assign Priority for Each Message Type
impl RBCMessage {
    pub fn priority(&self) -> u8 {
        match self {
            RBCMessage::RoundFinalized(_) => 0,  // ✅ **Highest priority**
            RBCMessage::Commit(_) => 1,  // **Commit is next**
            RBCMessage::Prevote(_) => 2, // **Prevote is after commit**
            RBCMessage::Proposal(_) => 3, // **Proposal is lowest priority**
        }
    }
}
