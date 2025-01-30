pub mod utils{
    pub mod merkle_utils; // Expose the structs module
    pub mod config_util;
    pub mod start_util; // Expose the structs module
    pub mod epoch_utils; // Expose the structs module
    pub mod dag_utils; // Expose the structs module
    pub mod errors_util; // Expose the structs module
    pub mod create_transaction_data;
}

pub mod structs{
    pub mod toml_config;
    pub mod requests;
    pub mod responses;
    pub mod node;
    pub mod dag;
}

pub mod handlers{
    pub mod handle_propose;
    pub mod handle_prevote;
    pub mod handle_commit;
    pub mod handle_dag_sync;
    pub mod handle_sync_epoch;
}

pub mod controllers{
    pub mod api_routes;
}

pub mod requests{
    pub mod send_proposals;
    pub mod send_prevotes;
    pub mod ip_server_requests;
    pub mod synchronize_epoch_across_nodes;

}