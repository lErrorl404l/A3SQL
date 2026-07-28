
#include "script_component.hpp"

class CfgPatches {
    class ADDON {
        name = "A3SQL - Admin";
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
