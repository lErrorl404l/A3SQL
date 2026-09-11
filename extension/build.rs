// Sync version from Cargo.toml — used by env!("CARGO_PKG_VERSION")
// This file exists to ensure CARGO_PKG_VERSION is the single version source
fn main() {
    println!("cargo:rerun-if-changed=Cargo.toml");
}
