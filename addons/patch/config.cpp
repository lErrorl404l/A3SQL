// Manual defines (avoiding CBA macro dependency for HEMTT compat)
#define ADDON a3sql_patch
#define COMPONENT_NAME "A3SQL - A3SQL_Patch"

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

class CfgEventHandlers {
    class ADDON {
        init = "call a3sql_patch_fnc_init";
    };
};

class Extended_PreInit_EventHandlers {
    class ADDON {
        init = "call a3sql_patch_fnc_settings";
    };
};

class Extended_PostInit_EventHandlers {
    class ADDON {
        init = "call a3sql_patch_fnc_postInit";
    };
};

class CfgFunctions {
    class a3sql {
        class patch {
            file = "z\a3sql\addons\patch";
            class init {};
            class settings {};
            class postInit {};
            class applyAll {};
            class applyRule {};
            class applyByTarget {};
            class registerHandler {};
            class reload {};
            class getRule {};
            class listRules {};
            class deleteRule {};
            class setDirty {};
            class opAdd {};
            class opSub {};
            class opMul {};
            class opDiv {};
            class opMod {};
            class opCat {};
            class opDefault {};
            class opRound {};
            class opClamp {};
            class opNegate {};
            class opReplace {};
            class opFormat {};
            class openEditor {};
            class gui_addRule {};
            class gui_editRule {};
            class gui_deleteRule {};
            class gui_listRules {};
            class gui_savePreset {};
            class gui_loadPreset {};
            class validateRule {};
            class applyGroup {};
            class activateGroup {};
            class deactivateGroup {};
            class applyOverrides {};
            class registerOverride {};
            class listOverrides {};
        };
    };
};

// Include dialog definition
#include "gui\config.hpp"
