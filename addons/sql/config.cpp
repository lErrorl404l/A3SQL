#include "script_component.hpp"

class CfgPatches {
    class ADDON {
        name = COMPONENT_NAME;
        author = "ABE Team";
        requiredVersion = 2.02;
        requiredAddons[] = {"a3db_main"};
        units[] = {};
        weapons[] = {};
    };
};

class CfgEventHandlers {
    class ADDON {
        init = "call a3db_fnc_init";
    };
};

class Extended_PreInit_EventHandlers {
    class ADDON {
        init = "call a3db_fnc_settings";
    };
};

class Extended_PostInit_EventHandlers {
    class ADDON {
        init = "call a3db_fnc_postInit";
    };
};

class CfgFunctions {
    class a3db {
        class a3db {
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
