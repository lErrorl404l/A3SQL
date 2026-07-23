// A3DB main — follows ACE3/CBA_A3 convention
#ifndef COMPONENT
    #define COMPONENT main
#endif
#ifndef COMPONENT_BEAUTIFIED
    #define COMPONENT_BEAUTIFIED A3DB_Main
#endif

#define MAINPREFIX z
#define PREFIX a3db

#include "script_version.hpp"

#define VERSION     MAJOR.MINOR
#define VERSION_STR MAJOR.MINOR.PATCHLVL.BUILD
#define VERSION_AR  MAJOR,MINOR,PATCHLVL,BUILD

#define REQUIRED_VERSION 2.02

#ifdef SUBCOMPONENT_BEAUTIFIED
    #define COMPONENT_NAME QUOTE(A3DB - COMPONENT_BEAUTIFIED - SUBCOMPONENT_BEAUTIFIED)
#else
    #define COMPONENT_NAME QUOTE(A3DB - COMPONENT_BEAUTIFIED)
#endif

#include "\x\cba\addons\main\script_mod.hpp"
