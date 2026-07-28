
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

class CfgFunctions {
    class a3sql {
        class patch_editor {
            file = "z\a3sql\addons\patch_editor\functions";
            class openEditor {};
            class gui_addRule {};
            class gui_editRule {};
            class gui_deleteRule {};
            class gui_listRules {};
            class gui_savePreset {};
            class gui_loadPreset {};
            class validateRule {};
        };
    };
};

// Include dialog definition
#include "ui\config.hpp"
