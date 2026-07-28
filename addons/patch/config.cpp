// Manual defines (avoiding CBA macro dependency for HEMTT compat)
#define ADDON a3sql_patch
#define COMPONENT_NAME "A3SQL - A3SQL_Patch"

#include "script_component.hpp"

class CfgPatches {
    class ADDON {
        name = COMPONENT_NAME;
        author = "ABE Team";
        requiredVersion = 2.02;
        requiredAddons[] = {"a3sql_database", "cba_xeh"};
        units[] = {};
        weapons[] = {};
    };
};

#include "CfgEventHandlers.hpp"

class CfgFunctions {
    class a3sql {
        class patch {
            file = "z\a3sql\addons\patch\functions";
            class init {};
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
