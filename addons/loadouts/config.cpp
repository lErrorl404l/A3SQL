// Manual defines (avoiding CBA macro dependency for HEMTT compat)
#define ADDON a3sql_loadouts
#define COMPONENT_NAME "A3SQL - A3SQL_Loadouts"

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
        class loadouts {
            file = "z\a3sql\addons\loadouts\functions";
            class init {};
            class createTemplate {};
            class getTemplate {};
            class listTemplates {};
            class listByFaction {};
            class deleteTemplate {};
            class applyLoadout {};
        };
    };
};
