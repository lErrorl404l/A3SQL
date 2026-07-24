// CBA script_macros.hpp stub for build-time resolution
// Minimal set of macros needed by addon config.cpp files

#ifndef INCLUDED_CBA_SCRIPT_MACROS
#define INCLUDED_CBA_SCRIPT_MACROS

#define ADDON COMPONENT
#ifndef COMPONENT_NAME
#define COMPONENT_NAME QUOTE(COMPONENT_BEAUTIFIED)
#endif

#define QUOTE(x) #x
#define QQUOTE(x) QUOTE(x)
#define DOUBLES(var1,var2) var1##_##var2
#define TRIPLES(var1,var2,var3) var1##_##var2##_##var3

#define GVAR(var) TRIPLES(COMPONENT,var)
#define QGVAR(var) QUOTE(GVAR(var))
#define FUNC(var) TRIPLES(COMPONENT,fnc,var)
#define QFUNC(var) QUOTE(FUNC(var))
#define DFUNC(var) DOUBLES(PREFIX,fnc_##var)

#define CSTRING(var) QUOTE(DOUBLES(COMPONENT,var))
#define ECSTRING(sys,var) QUOTE(DOUBLES(sys,var))

#define ARR_2(a,b) a,b
#define ARR_3(a,b,c) a,b,c
#define ARR_4(a,b,c,d) a,b,c,d

#define MP_EFFECT
#define PREP(var) FUNC(var) call EXT_PREP

#define VERSION_CONFIG version = VERSION; versionStr = QUOTE(VERSION); versionAr[] = {VERSION_MAJOR,VERSION_MINOR,VERSION_PATCH}

// Path macros — PREFIX set by script_mod.hpp or HEMTT auto-defines
#ifndef PREFIX
#define PREFIX a3db
#endif
#define DOUBLES_PREFIX(var1,var2) var1##_##var2
#define QPATHTO_FOLDER(var) QUOTE(z\PREFIX\addons\var)
#define QPATHTO_SYS(var) QUOTE(\z\PREFIX\addons\var)

#endif
