// Manual defines for HEMTT compat
#define ADDON a3sql_progression
#define COMPONENT_NAME "A3SQL - A3SQL_Progression"

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
        class progression {
            file = "z\a3sql\addons\progression\functions";
            class init {};
            class getProgression {};
            class updateRank {};
            class getHighestRank {};
            class processMissionEnd {};
            class queryStats {};
        };
    };
};
