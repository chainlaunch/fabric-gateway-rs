// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 fabric-gateway-rs contributors

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_root = PathBuf::from("proto");

    // Collect every .proto under proto/ so prost compiles the full dependency graph.
    let mut protos = Vec::new();
    collect_protos(&proto_root, &mut protos);

    tonic_build::configure()
        .build_server(false)
        .build_client(true)
        .compile_protos(&protos, &[proto_root])?;

    println!("cargo:rerun-if-changed=proto");
    Ok(())
}

fn collect_protos(dir: &PathBuf, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_protos(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("proto") {
            out.push(path);
        }
    }
}
