
#include "script_component.hpp"

class CfgPatches {
    class ADDON {
        name = "A3SQL - Patch Editor";
        author = "ABE Team";
        requiredVersion = REQUIRED_VERSION;
        requiredAddons[] = {"a3sql_patch_core", "cba_xeh"};
        units[] = {};
        weapons[] = {};
        VERSION_CONFIG;
    };
};

#include "CfgEventHandlers.hpp"


// Include dialog definition
// ui/config.hpp removed — window manager conflict
