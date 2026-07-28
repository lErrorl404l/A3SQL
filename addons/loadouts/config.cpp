// Manual defines (avoiding CBA macro dependency for HEMTT compat)
#define ADDON a3sql_loadouts
#define COMPONENT_NAME "A3SQL - A3SQL_Loadouts"

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
        init = "call a3sql_loadouts_fnc_init";
    };
};

class Extended_PreInit_EventHandlers {
    class ADDON {
        init = "call a3sql_loadouts_fnc_settings";
    };
};

class Extended_PostInit_EventHandlers {
    class ADDON {
        init = "call a3sql_loadouts_fnc_postInit";
    };
};

class CfgFunctions {
    class a3sql {
        class loadouts {
            file = "z\a3sql\addons\loadouts";
            class init {};
            class settings {};
            class postInit {};
            class createTemplate {};
            class getTemplate {};
            class listTemplates {};
            class listByFaction {};
            class deleteTemplate {};
            class applyLoadout {};
        };
    };
};
