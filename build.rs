use std::{env, path::PathBuf};

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    println!("OUT_DIR: {:?}", out_dir);

    tonic_build::compile_protos("protos/remote/upd88/com/remote.proto").unwrap();
}
