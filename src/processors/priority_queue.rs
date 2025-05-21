use std::cmp::Ordering;
use tracing::debug;
use crate::structs::requests::{CommitRequest, PrevoteRequest, ProposeRequest};

#[derive(Debug)]
pub enum RBCMessage {
    RoundFinalized(u64),
    Commit(CommitRequest),
    Prevote(PrevoteRequest),
    Proposal(ProposeRequest),
}

impl RBCMessage {
    pub fn priority(&self) -> u8 {
        match self {
            RBCMessage::RoundFinalized(_) => 0,
            RBCMessage::Commit(_) => 1,
            RBCMessage::Prevote(_) => 2,
            RBCMessage::Proposal(_) => 3,
        }
    }

    pub fn description(&self) -> String {
        match self {
            RBCMessage::RoundFinalized(r) => format!("RoundFinalized({})", r),
            RBCMessage::Commit(req) => format!("Commit(round={}, proposer={})", req.round_id, req.proposing_node_id),
            RBCMessage::Prevote(req) => format!(
                "Prevote(round={}, sender={})",
                req.proposals.get(0).map_or(0, |p| p.base.round_id),
                req.sender_url
            ),
            RBCMessage::Proposal(req) => format!(
                "Proposal(round={}, proposer={})",
                req.base.round_id,
                req.base.proposing_node_id
            ),
        }
    }

    pub fn log_enqueue(&self) {
        debug!("📥 RBCMessage in Queue: {} → priority {}", self.description(), self.priority());
    }

    pub fn log_process(&self) {
        debug!("⚙️ Processing RBCMessage: {} → priority {}", self.description(), self.priority());
    }

    pub fn type_str(&self) -> &'static str {
        match self {
            RBCMessage::RoundFinalized(_) => "rounds",
            RBCMessage::Commit(_) => "commits",
            RBCMessage::Prevote(_) => "prevotes",
            RBCMessage::Proposal(_) => "proposals",
        }
    }

    pub fn round_id(&self) -> Option<u64> {
        match self {
            RBCMessage::Commit(req) => Some(req.round_id),
            RBCMessage::Prevote(req) => req.proposals.get(0).map(|p| p.base.round_id),
            RBCMessage::Proposal(req) => Some(req.base.round_id),
            RBCMessage::RoundFinalized(_) => None,
        }
    }

    pub fn proposer_id(&self) -> Option<usize> {
        match self {
            RBCMessage::Commit(req) => Some(req.proposing_node_id as usize),
            RBCMessage::Prevote(req) => req.proposals.get(0).map(|p| p.base.proposing_node_id as usize),
            RBCMessage::Proposal(req) => Some(req.base.proposing_node_id as usize),
            RBCMessage::RoundFinalized(_) => None,
        }
    }
}

impl PartialEq for RBCMessage {
    fn eq(&self, other: &Self) -> bool {
        self.priority() == other.priority()
    }
}

impl Eq for RBCMessage {}

impl Ord for RBCMessage {
    fn cmp(&self, other: &Self) -> Ordering {
        other.priority().cmp(&self.priority())
    }
}

impl PartialOrd for RBCMessage {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
