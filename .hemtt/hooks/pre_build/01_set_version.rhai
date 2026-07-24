// Inject HEMTT project version into mod.cpp
// Replaces placeholder "0.0.0" with actual version from project.toml
let modcpp = HEMTT_VFS.join("mod.cpp").open_file().read();
modcpp.replace("0.0.0", HEMTT.project().version().to_string_short());
HEMTT_VFS.join("mod.cpp").create_file().write(modcpp);
print("mod.cpp version set to " + HEMTT.project().version().to_string_short());
