
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
            class openeditor {};            class gui_addrule {};            class gui_editrule {};            class gui_deleterule {};            class gui_listrules {};            class gui_savepreset {};            class gui_loadpreset {};            class validaterule {};        };
    };
};

// Include dialog definition
#include "ui\config.hpp"
