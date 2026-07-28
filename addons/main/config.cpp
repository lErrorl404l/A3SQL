// Manual defines for HEMTT compat
#define ADDON a3sql_main
#define COMPONENT_NAME "A3SQL - Main"

#include "script_component.hpp"

class CfgPatches {
    class ADDON {
        name = COMPONENT_NAME;
        author = "ABE Team";
        requiredVersion = 2.02;
        requiredAddons[] = {"cba_main", "cba_xeh"};
        units[] = {};
        weapons[] = {};
        forceRenameLib = "a3sql";
    };
};
