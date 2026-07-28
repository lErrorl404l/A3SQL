// Manual defines (avoiding CBA macro dependency for HEMTT compat)
#define ADDON a3sql_patch_operators
#define COMPONENT_NAME "A3SQL - A3SQL_Patch_Operators"

#include "script_component.hpp"

class CfgPatches {
    class ADDON {
        name = COMPONENT_NAME;
        author = "ABE Team";
        requiredVersion = 2.02;
        requiredAddons[] = {"a3sql_sql", "cba_xeh"};
        units[] = {};
        weapons[] = {};
    };
};

#include "CfgEventHandlers.hpp"

class CfgFunctions {
    class a3sql {
        class patch_operators {
            file = "z\a3sql\addons\patch_operators\functions";
            class applyOverrides {};
            class registerOverride {};
            class listOverrides {};
        };
    };
};
