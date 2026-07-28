
#include "script_component.hpp"

class CfgPatches {
    class ADDON {
        name = "A3SQL - Loadouts";
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
        class loadouts {
            file = "z\a3sql\addons\loadouts\functions";
            class init {};
            class createtemplate {};            class gettemplate {};            class listtemplates {};            class listbyfaction {};            class deletetemplate {};            class applyloadout {};        };
    };
};
