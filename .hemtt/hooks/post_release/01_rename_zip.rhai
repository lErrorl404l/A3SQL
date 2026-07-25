// post_release: rename ZIP to a3db_X.Y.Z.zip
let prefix = HEMTT.project().name();
let version = HEMTT.project().version().to_string_short();
let new_name = prefix + "_" + version + ".zip";
let dir = HEMTT.project().directory().to_string();
let old = dir + "/releases/" + prefix + ".zip";
if std::fs::exists(old) {
    std::fs::rename(old, dir + "/releases/" + new_name);
    print("Renamed release to " + new_name);
}
