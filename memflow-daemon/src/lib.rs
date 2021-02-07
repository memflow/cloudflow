pub mod config;
pub use config::*;

pub mod error;

pub mod memflow_rpc {
    tonic::include_proto!("memflow_rpc");
}
