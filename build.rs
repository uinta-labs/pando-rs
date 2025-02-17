use std::{env, path::PathBuf};

fn main() {
    // If using protobuf-src (see Cargo.toml)t
    // std::env::set_var("PROTOC", protobuf_src::protoc());

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    println!("OUT_DIR: {:?}", out_dir);

    tonic_build::compile_protos("protos/remote/upd88/com/remote.proto").unwrap();
}
