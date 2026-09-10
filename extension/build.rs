// Sync version from Cargo.toml — used by env!("CARGO_PKG_VERSION")
// This file exists to ensure CARGO_PKG_VERSION is the single version source
fn main() {
    println!("cargo:rerun-if-changed=Cargo.toml");

    // 32-bit Arma 3 Publisher requires decorated __stdcall exports
    // (_RVExtension@12 etc). Win32.def maps these to the undecorated
    // Rust symbols. Only emitted for i686 Windows targets.
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.starts_with("i686-pc-windows") {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let def_path = format!("{}/Win32.def", manifest_dir);
        println!("cargo:rustc-link-arg={}", def_path);
    }
}
