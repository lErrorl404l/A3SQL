#include "script_component.hpp"

class CfgPatches {
    class ADDON {
        name = COMPONENT_NAME;
        author = "lErrorl404l";
        requiredVersion = REQUIRED_VERSION;
        requiredAddons[] = {"a3sql_database", "cba_xeh"};
        units[] = {};
        weapons[] = {};
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

#include "CfgEventHandlers.hpp"

