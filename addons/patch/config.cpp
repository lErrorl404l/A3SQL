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
            file = QPATHTO_FOLDER(patch);
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
        };
    };
};
