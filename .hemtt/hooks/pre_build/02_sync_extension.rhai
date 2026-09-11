// Sync freshly-built extension binaries into the build with Arma 3 naming.
// Local `cargo build --release` emits liba3sql.so (or a3sql.dll on Windows);
// Arma expects a3sql_x64.so / a3sql_x64.dll (forceRenameLib = "a3sql").
//
// With the root workspace Cargo.toml, cargo outputs to target/ (root level).
// Without it, cargo outputs to extension/target/. This hook searches both.
// In CI the binaries are already renamed correctly before `hemtt build`,
// so the hook is a no-op there. Skipped sources are reported, not fatal.
let root_target = HEMTT_VFS.join("target");
let ext_target = HEMTT_VFS.join("extension").join("target");

let synced = 0;

// Linux x86_64 — try root target first, then extension target
let linux64 = root_target.join("release").join("liba3sql.so");
if !linux64.exists() { linux64 = ext_target.join("release").join("liba3sql.so"); }
if linux64.exists() {
    let dest = HEMTT_VFS.join("a3sql_x64.so");
    if dest.exists() { dest.remove_file(); }
    linux64.copy(dest);
    synced += 1;
    print("Synced a3sql_x64.so (linux x86_64)");
}

// Windows x64 (MSVC only)
let win64 = root_target.join("x86_64-pc-windows-msvc").join("release").join("a3sql.dll");
if !win64.exists() { win64 = ext_target.join("x86_64-pc-windows-msvc").join("release").join("a3sql.dll"); }
if win64.exists() {
    let dest = HEMTT_VFS.join("a3sql_x64.dll");
    if dest.exists() { dest.remove_file(); }
    win64.copy(dest);
    synced += 1;
    print("Synced a3sql_x64.dll (windows x86_64 MSVC)");
}

if synced == 0 {
    print("No extension binaries found in target/ — build will package any repo-root copies or none.");
}