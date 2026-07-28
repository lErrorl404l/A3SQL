// A3SQL script_macros.hpp — follows ACE3/CBA_A3 convention
// Includes CBA macros via script_macros_common.hpp, then adds project-specific ones.

#include "\x\cba\addons\main\script_macros_common.hpp"
#include "\x\cba\addons\xeh\script_xeh.hpp"

#define DFUNC(var1) TRIPLES(PREFIX,fnc,var1)

#undef QFUNC
#undef QEFUNC
#define QFUNC(var1) QUOTE(DFUNC(var1))
#define QEFUNC(var1,var2) QUOTE(DEFUNC(var1,var2))

#define GETMVAR(var1,var2) (missionNamespace getVariable [ARR_2(QUOTE(var1),var2)])
#define SETMVAR(var1,var2) missionNamespace setVariable [ARR_2(QUOTE(var1),var2)]
#define SETMPVAR(var1,var2) missionNamespace setVariable [ARR_3(QUOTE(var1),var2,true)]

#undef GETVAR
#define GETVAR(var1,var2,var3) (var1 getVariable [ARR_3(QUOTE(var2),var3)])

#undef SETVAR
#define SETVAR(var1,var2,var3) var1 setVariable [ARR_3(QUOTE(var2),var3)]

#undef PREP
#define PREP(fncName) [QPATHTOF(functions\DOUBLES(fnc,fncName).sqf), QFUNC(fncName)] call CBA_fnc_compileFunction

#define VERSION_CONFIG version = VERSION; versionStr = QUOTE(VERSION); versionAr[] = {VERSION_MAJOR,VERSION_MINOR,VERSION_PATCH}

#define MP_EFFECT
