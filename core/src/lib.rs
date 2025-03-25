pub mod config;
pub mod config_json;
pub mod config_txt;
pub mod daemon;
pub mod mqtt;
pub mod registration;
pub mod schedule;
pub mod temp;

pub mod grpc_remote {
    tonic::include_proto!("remote.upd88.com");
}
