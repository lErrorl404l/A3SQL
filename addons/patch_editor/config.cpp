// Manual defines (avoiding CBA macro dependency for HEMTT compat)
#define ADDON a3sql_patch_editor
#define COMPONENT_NAME "A3SQL - A3SQL_Patch_Editor"

#include "script_component.hpp"

class CfgPatches {
    class ADDON {
        name = COMPONENT_NAME;
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
