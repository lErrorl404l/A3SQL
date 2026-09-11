
#include "script_component.hpp"

class CfgPatches {
    class ADDON {
        name = "A3SQL";
        author = "lErrorl404l";
        requiredVersion = REQUIRED_VERSION;
        requiredAddons[] = {"cba_main", "cba_xeh"};
        units[] = {};
        weapons[] = {};
        forceRenameLib = "a3sql";
        VERSION_CONFIG;
    };
};

// Register with CBA's versioning system so other mods can check our
// version via CBA_fnc_checkCompat.
class CfgSettings {
    class CBA {
        class Versioning {
            class PREFIX {};
        };
    };
};
