#include "script_mod.hpp"

class CfgPatches {
    class a3sql_main {
        name = "A3DB - Main";
        author = "ABE Team";
        requiredVersion = 2.02;
        requiredAddons[] = {"cba_main", "cba_xeh"};
        units[] = {};
        weapons[] = {};
        forceRenameLib = "a3sql";
    };
};
