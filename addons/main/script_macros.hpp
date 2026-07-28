#ifndef DEBUG_SYNCHRONOUS
#define DEBUG_SYNCHRONOUS
#endif
#include "\x\cba\addons\main\script_macros_common.hpp"

// a3sql-specific macros — unique names to avoid PW1 redefinition warnings
#define ADDON DOUBLES(PREFIX,COMPONENT)
#define A3FUNC(var1) TRIPLES(PREFIX,COMPONENT,fnc_##var1)
#define A3DEFUNC(var1,var2) TRIPLES(DOUBLES(PREFIX,var1),fnc,var2)
#define A3QFUNC(var1) QUOTE(A3FUNC(var1))
#define A3QEFUNC(var1,var2) QUOTE(A3DEFUNC(var1,var2))
#define A3PREP(fncName) [QPATHTOF(functions\DOUBLES(fnc,fncName).sqf), A3QFUNC(fncName)] call CBA_fnc_compileFunction
#define A3VERSION_CONFIG version = VERSION; versionStr = QUOTE(VERSION); versionAr[] = {VERSION_MAJOR,VERSION_MINOR,VERSION_PATCH}
