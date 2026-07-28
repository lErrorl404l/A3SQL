
#include "script_component.hpp"

class CfgPatches {
    class ADDON {
        name = "A3SQL - Patch Core";
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
        class patch_core {
            file = "z\a3sql\addons\patch_core\functions";
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
            class applyGroup {};
            class activateGroup {};
            class deactivateGroup {};
        };
    };
};
