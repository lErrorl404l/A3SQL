#include "script_component.hpp"

class CfgPatches {
    class ADDON {
        name = COMPONENT_NAME;
        author = "ABE Team";
        requiredVersion = 2.02;
        requiredAddons[] = {"a3sql_main"};
        units[] = {};
        weapons[] = {};
    };
};

class CfgEventHandlers {
    class ADDON {
        init = "call a3sql_fnc_init";
    };
};

class Extended_PreInit_EventHandlers {
    class ADDON {
        init = "call a3sql_fnc_settings";
    };
};

class Extended_PostInit_EventHandlers {
    class ADDON {
        init = "call a3sql_fnc_postInit";
    };
};

class CfgFunctions {
    class a3sql {
        class a3sql {
            file = QPATHTO_FOLDER(sql);
            class init {};
            class settings {};
            class postInit {};
            class execute {};
            class loadJSON {};
            class dumpSQL {};
            class exportJSON {};
            class exportCSV {};
            class exportSQL {};
            class save {};
            class load {};
        };
    };
};
