
#include "script_component.hpp"

class CfgPatches {
    class ADDON {
        name = "A3SQL";
        author = "ABE Team";
        requiredVersion = REQUIRED_VERSION;
        requiredAddons[] = {"cba_main", "cba_xeh"};
        units[] = {};
        weapons[] = {};
        forceRenameLib = "a3sql";
        VERSION_CONFIG;
    };
};
