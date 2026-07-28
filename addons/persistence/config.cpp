// Manual defines for HEMTT compat
#define ADDON a3sql_persistence
#define COMPONENT_NAME "A3SQL - A3SQL_Persistence"

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

class CfgEventHandlers {
    class ADDON {
        init = "call a3sql_persistence_fnc_init";
    };
};

class Extended_PreInit_EventHandlers {
    class ADDON {
        init = "call a3sql_persistence_fnc_settings";
    };
};

class Extended_PostInit_EventHandlers {
    class ADDON {
        init = "call a3sql_persistence_fnc_postInit";
    };
};

class CfgFunctions {
    class a3sql {
        class persistence {
            file = "z\a3sql\addons\persistence";
            class init {};
            class settings {};
            class postInit {};
            class savePlayer {};
            class restorePlayer {};
            class handleDisconnect {};
            class handleJIP {};
        };
    };
};
