// Manual defines for HEMTT compat
#define ADDON a3sql_progression
#define COMPONENT_NAME "A3SQL - A3SQL_Progression"

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
        init = "call a3sql_progression_fnc_init";
    };
};

class Extended_PreInit_EventHandlers {
    class ADDON {
        init = "call a3sql_progression_fnc_settings";
    };
};

class Extended_PostInit_EventHandlers {
    class ADDON {
        init = "call a3sql_progression_fnc_postInit";
    };
};

class CfgFunctions {
    class a3sql {
        class progression {
            file = "z\a3sql\addons\progression";
            class init {};
            class settings {};
            class postInit {};
            class getProgression {};
            class updateRank {};
            class getHighestRank {};
            class processMissionEnd {};
            class queryStats {};
        };
    };
};
