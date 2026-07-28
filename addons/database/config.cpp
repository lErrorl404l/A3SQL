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

#include "CfgEventHandlers.hpp"

class CfgFunctions {
    class a3sql {
        class a3sql {
            file = "z\a3sql\addons\database\functions";
            class init {};
            class execute {};
            class loadJSON {};
            class dumpSQL {};
            class exportJSON {};
            class exportCSV {};
            class exportSQL {};
            class save {};
            class load {};
            class executePrepared {};
            class executeTimed {};
            class prepare {};
            class selectAll {};
            class selectArray {};
            class selectMap {};
        };
    };
};
