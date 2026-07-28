
class CfgPatches {
    class ADDON {
        name = "A3SQL - Analytics";
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
