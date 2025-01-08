pub mod generate_keys;

pub mod utils{
    pub mod merkle_utils; // Expose the structs module
    pub mod config_util;
    pub mod ip_server_utils; // Expose the structs module
    pub mod rbc_utils; // Expose the structs module
    pub mod epoch_utils; // Expose the structs module
}

pub mod structs{
    pub mod toml_config;
    pub mod requests;
    pub mod responses;
    pub mod node;
}

pub mod handlers{
    pub mod handle_propose;
    pub mod handle_prevote;
    pub mod handle_commit;
}
