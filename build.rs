// Recompile le binaire quand le build du frontend change (il y est embarqué via rust-embed).
fn main() {
    println!("cargo:rerun-if-changed=frontend/dist");
}
