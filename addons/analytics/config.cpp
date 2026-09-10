#include "script_component.hpp"

class CfgPatches {
    class ADDON {
        name = "A3SQL - Analytics";
        author = "lErrorl404l";
        requiredVersion = REQUIRED_VERSION;
        requiredAddons[] = {"a3sql_database", "cba_xeh"};
        units[] = {};
        weapons[] = {};
        VERSION_CONFIG;
    };
};

#include "CfgEventHandlers.hpp"

