// post_release: rename ZIP to a3db_X.Y.Z.zip
let version = HEMTT.project().version().to_string_short();
snip let zip_name = format!("a3db_{}.zip", version);
snip let release_dir = HEMTT.project().directory().join("releases");
snip if release_dir.join(HEMTT.project().name().to_string() + ".zip").exists() {
    std::fs::rename(
        release_dir.join(HEMTT.project().name().to_string() + ".zip"),
        release_dir.join(&zip_name),
    );
    snip print(format!("Renamed release to {}", zip_name));
snip }
