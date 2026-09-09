// Sync freshly-built extension binaries into the build with Arma 3 naming.
// Local `cargo build --release` emits liba3sql.so (or a3sql.dll on Windows);
// Arma expects a3sql_x64.so / a3sql_x64.dll (forceRenameLib = "a3sql").
//
// This hook copies whatever exists in extension/target/* into the VFS root,
// so `hemtt build`/`hemtt release` always package the current build — never a
// stale repo-root dev copy. In CI the binaries are already renamed correctly
// before `hemtt build`, so the hook is a no-op there. Skipped sources are
// reported, not fatal: a hook must never break a build that has no binaries.
let target = HEMTT_VFS.join("extension").join("target");

let synced = 0;

// Linux x86_64
let linux64 = target.join("release").join("liba3sql.so");
if linux64.exists() {
    let dest = HEMTT_VFS.join("a3sql_x64.so");
    if dest.exists() { dest.remove_file(); }
    linux64.copy(dest);
    synced += 1;
    print("Synced a3sql_x64.so (linux x86_64)");
}

// Linux i686
let linux32 = target.join("i686-unknown-linux-gnu").join("release").join("liba3sql.so");
if linux32.exists() {
    let dest = HEMTT_VFS.join("a3sql.so");
    if dest.exists() { dest.remove_file(); }
    linux32.copy(dest);
    synced += 1;
    print("Synced a3sql.so (linux i686)");
}

// Windows x86_64
let win64 = target.join("x86_64-pc-windows-gnu").join("release").join("a3sql.dll");
if win64.exists() {
    let dest = HEMTT_VFS.join("a3sql_x64.dll");
    if dest.exists() { dest.remove_file(); }
    win64.copy(dest);
    synced += 1;
    print("Synced a3sql_x64.dll (windows x86_64)");
}

// Windows i686
let win32 = target.join("i686-pc-windows-gnu").join("release").join("a3sql.dll");
if win32.exists() {
    let dest = HEMTT_VFS.join("a3sql.dll");
    if dest.exists() { dest.remove_file(); }
    win32.copy(dest);
    synced += 1;
    print("Synced a3sql.dll (windows i686)");
}

if synced == 0 {
    print("No extension binaries found in extension/target — build will package any repo-root copies or none.");
}