#include "script_component.hpp"

class CfgPatches {
    class ADDON {
        name = "A3SQL - Database";
        author = "ABE Team";
        requiredVersion = REQUIRED_VERSION;
        requiredAddons[] = {"a3sql_main", "cba_xeh"};
        units[] = {};
        weapons[] = {};
        VERSION_CONFIG;
    };
};

#include "CfgEventHandlers.hpp"

