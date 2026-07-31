#include "../script_component.hpp"

/*
 * Escapes single quotes for safe SQL string interpolation.
 * Shared helper — same pattern as fnc_saveplayer.sqf, extracted so every
 * addon can use it. Non-string input falls back to "" via the default param.
 */

params [["_s", "", [""]]];

_s regexReplace ["'", "''"]
