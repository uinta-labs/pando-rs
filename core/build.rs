use std::{env, path::PathBuf};

fn main() {
    // If using protobuf-src (see Cargo.toml)t
    // std::env::set_var("PROTOC", protobuf_src::protoc());

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    println!("OUT_DIR: {:?}", out_dir);

    // tonic_build::compile_protos("protos/remote/upd88/com/remote.proto").unwrap();

    // tonic_build::configure()
    //     .build_server(true)
    //     .compile_protos(
    //         &[
    //             "protos/remote/upd88/com/remote.proto",
    //             "protos/remote/upd88/com/device.proto",
    //         ],
    //         &["protos/remote/upd88/com"],
    //     )
    //     .unwrap();

    // tonic_build::configure()
    //     .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
    //     .type_attribute(".", "#[serde(default)]")
    //     .type_attribute(".", "#[serde(rename_all = \"camelCase\")]")
    //     .compile_protos(
    //         &["protos/remote/upd88/com/types.proto"],
    //         &["protos/remote/upd88/com"],
    //     )
    //     .unwrap();

    // tonic_build::configure()
    //     .build_server(true)
    //     .compile_protos(
    //         &["protos/remote/upd88/com"],
    //     )
    //     .unwrap();

    tonic_build::configure()
        .build_server(true)
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .type_attribute(".", "#[serde(rename_all = \"camelCase\")]")
        .compile_protos(
            &[
                "protos/remote/upd88/com/types.proto",
                "protos/remote/upd88/com/remote.proto",
                "protos/remote/upd88/com/device.proto",
            ],
            &["protos/remote/upd88/com"],
        )
        .unwrap();
}
