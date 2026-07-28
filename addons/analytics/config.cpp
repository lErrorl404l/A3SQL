// Manual defines for HEMTT compat
#define ADDON a3sql_analytics
#define COMPONENT_NAME "A3SQL - A3SQL_Analytics"

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
        class analytics {
            file = "z\a3sql\addons\analytics\functions";
            class init {};
            class handleKilled {};
            class handleFiredMan {};
            class flushShotBuffer {};
            class exportAnalytics {};
            class queryKills {};
            class takeSnapshot {};
            class exportSession {};
            class getSnapshotCount {};
        };
    };
};
