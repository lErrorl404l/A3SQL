#include "script_component.hpp"

class CfgPatches {
    class ADDON {
        name = "A3SQL - Database";
        author = "ABE Team";
        requiredVersion = 2.02;
        requiredAddons[] = {"a3sql_main", "cba_xeh"};
        units[] = {};
        weapons[] = {};
    };
};

#include "CfgEventHandlers.hpp"

