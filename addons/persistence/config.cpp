// Manual defines for HEMTT compat
#define ADDON a3sql_persistence
#define COMPONENT_NAME "A3SQL - A3SQL_Persistence"

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
