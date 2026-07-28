#include "script_component.hpp"

class CfgPatches {
    class ADDON {
        name = "A3SQL - Database";
        author = "ABE Team";
        requiredVersion = 2.02;
        requiredAddons[] = {"a3sql_main"};
        units[] = {};
        weapons[] = {};
    };
};

#include "CfgEventHandlers.hpp"

class CfgFunctions {
    class a3sql {
        class a3sql {
            file = "z\a3sql\addons\database\functions";
            class init {};
            class execute {};            class loadjson {};            class dumpsql {};            class exportjson {};            class exportcsv {};            class exportsql {};            class save {};            class load {};            class executeprepared {};            class executetimed {};            class prepare {};            class selectall {};            class selectarray {};            class selectmap {};        };
    };
};
