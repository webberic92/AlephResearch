use std::cmp::Ordering;
use std::collections::BinaryHeap;
use crate::structs::requests::{CommitRequest, PrevoteRequest, ProposeRequest};

/// Assign priorities: **Lower value means higher priority**
#[derive(Debug)]
pub enum RBCMessage {
    Commit(CommitRequest),  // Priority 1
    Prevote(PrevoteRequest), // Priority 2
    Proposal(ProposeRequest), // Priority 3
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
            RBCMessage::Commit(_) => 1,  // **Highest priority**
            RBCMessage::Prevote(_) => 2, // **Medium priority**
            RBCMessage::Proposal(_) => 3, // **Lowest priority**
        }
    }
}