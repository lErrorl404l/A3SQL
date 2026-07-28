
#include "script_component.hpp"

class CfgPatches {
    class ADDON {
        name = "A3SQL - Patch Editor";
        author = "ABE Team";
        requiredVersion = 2.02;
        requiredAddons[] = {"a3sql_patch_core", "cba_xeh"};
        units[] = {};
        weapons[] = {};
    };
};

#include "CfgEventHandlers.hpp"


// Include dialog definition
#include "ui\config.hpp"
