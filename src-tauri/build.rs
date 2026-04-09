fn main() {
    tauri_build::build();
    println!("cargo:rerun-if-changed=tauri.conf.json");
}

