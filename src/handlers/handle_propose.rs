use std::sync::Arc;
use base64::{engine::general_purpose, Engine};
use tokio::sync::RwLock;
use tracing::{error, info};
use crate::{
    handlers::handle_prevote::handle_prevote,
    structs::{
        node::Node,
        requests::{PrevoteRequest, ProposeRequest},
    },
    utils::dag_utils::{check_size, ensure_dag_round_sync},
};

/*
**ch-RBC Proof Validation for `handle_propose`**
--------------------------------------------------

7: Upon receiving `propose(h, b_j, s_j)` from `P_s`
   - The function `handle_propose` is called upon receiving a `ProposeRequest` from another node.

8: If `received_propose(P_i, r)` then terminate
   - The proposal tracker is checked to ensure no duplicate proposals from the same sender (`P_s`) in the same epoch (`r`).
   - If a duplicate exists, the function returns early.

9: If `check_size(s_j)` then
   - The function `check_size(&decoded_shards)` verifies the shard sizes.
   - If the size is invalid, the function returns early.

10: Wait until `D_i` reaches round `r-1`
   - The function `ensure_dag_round_sync(node.clone(), epoch_id).await?;` ensures the DAG is synchronized to `r-1` before proceeding.

11: Multicast `prevote(h, b_j, s_j)`
   - If enough proposals are received (`proposal_count >= required_proposals`), prevotes are sent for each stored proposal.

12: `received_propose(P_i, r) = True`
   - The proposal is stored in `proposal_tracker` using `update_proposal_tracker()`, ensuring the proposal is marked as received.

13: received_propose(Pi , r ) = True
*/

// Step 7: Upon receiving `propose(h, b_j, s_j)` from `P_s`
// - This function `handle_propose` is called when a proposal message is received.
pub async fn handle_propose(
    node: Arc<RwLock<Node>>,
    propose_request: ProposeRequest,
) -> Result<(), String> {
    let node_id;
    let epoch_id = propose_request.base.epoch_id;

    // Step 8: If `received_propose(P_i, r)` then terminate
    // - Check if a proposal from the same sender (`P_s`) has already been received for this epoch (`r`).
    // - If it has, terminate early to prevent duplicate processing.
    {
        let proposal_tracker;
        {
            let node_read = node.read().await;
            node_id = node_read.id;
            info!(
                "Node {}: Handling Propose for epoch {} from sender {}",
                node_id, propose_request.base.epoch_id, propose_request.base.proposing_node_id
            );
            proposal_tracker = node_read.proposal_tracker.clone();
        }

        let proposal_tracker_read = proposal_tracker.lock().await;


        if let Some(epoch_proposals) = proposal_tracker_read.get(&epoch_id) {
            if epoch_proposals.contains_key(&propose_request.base.proposing_node_id) {
                info!(
                    "Node {}: Already received propose for epoch {} from Node {}. Terminating.",
                    node_id, epoch_id, propose_request.base.proposing_node_id
                );
                return Ok(());
            }
        }
    }

    // Step 9: If `check_size(s_j)` then
    // - Validate the size of the received shard.
    // - If the shard size is invalid, reject the proposal.
    let decoded_shards: Vec<Vec<u8>> = propose_request
        .shards
        .iter()
        .map(|shard| general_purpose::STANDARD.decode(shard.as_bytes()))
        .collect::<Result<Vec<Vec<u8>>, _>>()
        .map_err(|e| format!("Failed to decode shards: {:?}", e))?;


    if !check_size(&decoded_shards) {
        return Err(format!(
            "Node {}: Received oversized unit, rejecting propose.",
            node_id
        ));
    }

    // Step 10: Wait until `D_i` reaches round `r-1`
    // - Ensure the DAG is synchronized to at least round `r-1` before processing the proposal.
    ensure_dag_round_sync(node.clone(), epoch_id).await?;

    info!("Node {}: Checked sizes and ensured DAG round sync successfully.", node_id);

    // Step 12: `received_propose(P_i, r) = True`
    // - Store the received proposal in the `proposal_tracker` to prevent reprocessing.
    let (proposal_count, required_proposals, stored_proposals) =
        Node::update_proposal_tracker(node.clone(), propose_request.clone()).await?;

        let proposal_tracker;
        {
        let node_read = node.read().await;

        proposal_tracker = node_read.proposal_tracker.clone();

        let proposal_tracker_read = proposal_tracker.lock().await;

        info!(
            "Node {}: Current Proposal Tracker for epoch {}: {:?}",
            node_id, epoch_id, proposal_tracker_read
        );
        }   

    // Step 11: Multicast `prevote(h, b_j, s_j)`
    // - If the number of received proposals reaches the required threshold, proceed to prevote.
    if proposal_count >= required_proposals {

        info!(
            "Node {}: Received enough proposals ({}/{}) for epoch {}. Transitioning to prevote.",
            node_id, proposal_count, required_proposals, propose_request.base.epoch_id
        );

        for stored_propose in stored_proposals {
            info!(
                "Node {}: Sending prevote for transaction proposed by Node {}",
                node_id, stored_propose.base.proposing_node_id
            );

            let prevote_request = {
                let node_read = node.read().await;
                PrevoteRequest {
                    propose: stored_propose.clone(),
                    sender_url: node_read.ip_address.clone(),
                }
            };

            handle_prevote(node.clone(),prevote_request).await.map_err(|e| {
                error!(
                    "Node {}: Failed to handle prevote for epoch {}. Error: {:?}",
                    node_id, stored_propose.base.epoch_id, e
                );
                format!("Prevote phase failed: {:?}", e)
            })?;
        }
    }

    //13: received_propose(Pi , r ) = True
    info!(
        "Node {}: Proposal successfully handled for epoch {} from sender {}",
        node_id, propose_request.base.epoch_id, propose_request.base.proposing_node_id
    );
    Ok(())
}

