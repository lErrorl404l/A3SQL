
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
            class applyall {};            class applyrule {};            class applybytarget {};            class registerhandler {};            class reload {};            class getrule {};            class listrules {};            class deleterule {};            class setdirty {};            class opadd {};            class opsub {};            class opmul {};            class opdiv {};            class opmod {};            class opcat {};            class opdefault {};            class opround {};            class opclamp {};            class opnegate {};            class opreplace {};            class opformat {};            class applygroup {};            class activategroup {};            class deactivategroup {};        };
    };
};
