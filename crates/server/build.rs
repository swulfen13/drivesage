// Make sure the embedded UI folder exists so the crate compiles even before
// the web UI has been built (it then shows a hint instead of the UI).
fn main() {
    let dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/dist");
    if !dist.exists() {
        let _ = std::fs::create_dir_all(&dist);
    }
    println!("cargo:rerun-if-changed=../../web/dist");
}
