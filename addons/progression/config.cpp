
#include "script_component.hpp"

class CfgPatches {
    class ADDON {
        name = "A3SQL - Progression";
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
            class getprogression {};            class updaterank {};            class gethighestrank {};            class processmissionend {};            class querystats {};        };
    };
};
