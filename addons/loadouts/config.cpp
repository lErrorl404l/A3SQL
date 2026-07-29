
#include "script_component.hpp"

class CfgPatches {
    class ADDON {
        name = "A3SQL - Loadouts";
        author = "ABE Team";
        requiredVersion = REQUIRED_VERSION;
        requiredAddons[] = {"a3sql_database", "cba_xeh"};
        units[] = {};
        weapons[] = {};
        VERSION_CONFIG;
    };
};

#include "CfgEventHandlers.hpp"

