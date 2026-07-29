#ifndef DEBUG_SYNCHRONOUS
#define DEBUG_SYNCHRONOUS
#endif
#include "\x\cba\addons\main\script_macros_common.hpp"

// Explicit ADDON — CBA's #ifndef guard in script_macros_common.hpp may not
// always fire in HEMTT's cpp preprocessor context, so define it ourselves.
#ifndef ADDON
#define ADDON DOUBLES(PREFIX,COMPONENT)
#endif

// a3sql-specific macros — unique names to avoid PW1 redefinition warnings
#define A3FUNC(var1) TRIPLES(PREFIX,COMPONENT,fnc_##var1)
#define A3DEFUNC(var1,var2) TRIPLES(DOUBLES(PREFIX,var1),fnc,var2)
#define A3QFUNC(var1) QUOTE(A3FUNC(var1))
#define A3QEFUNC(var1,var2) QUOTE(A3DEFUNC(var1,var2))

#ifdef DISABLE_COMPILE_CACHE
#define A3PREP(fncName) A3FUNC(fncName) = compile preprocessFileLineNumbers QPATHTOF(functions\DOUBLES(fnc,fncName).sqf)
#else
#define A3PREP(fncName) [QPATHTOF(functions\DOUBLES(fnc,fncName).sqf), A3QFUNC(fncName)] call CBA_fnc_compileFunction
#endif
