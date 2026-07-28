#ifndef DEBUG_SYNCHRONOUS
#define DEBUG_SYNCHRONOUS
#endif
#include "\x\cba\addons\main\script_macros_common.hpp"

#undef DFUNC
#define DFUNC(var1) TRIPLES(PREFIX,fnc,var1)
#undef DEFUNC
#define DEFUNC(var1,var2) TRIPLES(DOUBLES(PREFIX,var1),fnc,var2)

#undef QFUNC
#undef QEFUNC
#define QFUNC(var1) QUOTE(DFUNC(var1))
#define QEFUNC(var1,var2) QUOTE(DEFUNC(var1,var2))

#undef PREP
#define PREP(fncName) [QPATHTOF(functions\DOUBLES(fnc,fncName).sqf), QFUNC(fncName)] call CBA_fnc_compileFunction

#undef VERSION_CONFIG
#define VERSION_CONFIG version = VERSION; versionStr = QUOTE(VERSION); versionAr[] = {VERSION_MAJOR,VERSION_MINOR,VERSION_PATCH}
