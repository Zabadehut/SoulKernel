fn main() {
    println!("cargo:rerun-if-changed=scripts/prepare_embedded_python.py");
    println!("cargo:rerun-if-changed=runtime/python/README.md");
    tauri_build::build()
}
