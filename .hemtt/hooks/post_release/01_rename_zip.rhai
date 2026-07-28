// post_release: rename ZIP to a3sql_X.Y.Z.zip
let prefix = HEMTT.project().name();
let version = HEMTT.project().version().to_string_short();
print("Release ready: " + prefix + "_" + version + ".zip");
