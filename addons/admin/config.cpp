// Manual defines (avoiding CBA macro dependency for HEMTT compat)
#define ADDON a3sql_admin
#define COMPONENT_NAME "A3SQL - A3SQL_Admin"

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
        class admin {
            file = "z\a3sql\addons\admin\functions";
            class init {};
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
