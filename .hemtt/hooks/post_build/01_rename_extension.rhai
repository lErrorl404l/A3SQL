// Rename cargo-named extension binary to the Arma 3 convention.
// Local `cargo build` emits liba3sql.so; 64-bit Arma looks for a3sql_x64.so
// (see community.bistudio.com/wiki/callExtension — the game appends _x64
// based on process bitness). CI renames before `hemtt release`; this hook
// keeps local `hemtt build`/`hemtt release` output loadable too.
let cargo_so = HEMTT_VFS.join("liba3sql.so");
if cargo_so.exists() {
    let target = HEMTT_VFS.join("a3sql_x64.so");
    if target.exists() {
        // Correctly-named binary already present (repo-root dev copy) —
        // drop the cargo-named duplicate instead of overwriting.
        cargo_so.remove_file();
        print("Removed duplicate liba3sql.so (a3sql_x64.so already present)");
    } else {
        cargo_so.move(target);
        print("Renamed liba3sql.so -> a3sql_x64.so");
    }
}
