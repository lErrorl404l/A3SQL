
#include "script_component.hpp"

class CfgPatches {
    class ADDON {
        name = "A3SQL - Persistence";
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
        class persistence {
            file = "z\a3sql\addons\persistence\functions";
            class init {};
            class savePlayer {};
            class restorePlayer {};
            class handleDisconnect {};
            class handleJIP {};
        };
    };
};
