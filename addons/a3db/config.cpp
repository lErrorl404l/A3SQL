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

class CfgFunctions {
    class a3db {
        class a3db {
            file = QPATHTO_FOLDER(a3db);
            class init {};
            class execute {};
            class parseResult {};
            class loadJSON {};
            class dumpSQL {};
        };
    };
};
