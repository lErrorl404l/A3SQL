// Manual defines (avoiding CBA macro dependency for HEMTT compat)
#define ADDON a3sql_admin
#define COMPONENT_NAME "A3SQL - A3SQL_Admin"

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
        init = "call a3sql_admin_fnc_init";
    };
};

class Extended_PreInit_EventHandlers {
    class ADDON {
        init = "call a3sql_admin_fnc_settings";
    };
};

class Extended_PostInit_EventHandlers {
    class ADDON {
        init = "call a3sql_admin_fnc_postInit";
    };
};

        class CfgFunctions {
    class a3sql {
        class admin {
            file = "z\a3sql\addons\admin";
            class init {};
            class settings {};
            class postInit {};
            class executeCommand {};
            class addCommand {};
            class listCommands {};
            class clearExecuted {};
            class listPlayers {};
            class getPlayer {};
            class kickPlayer {};
            class banPlayer {};
        };
    };
};
