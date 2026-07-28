// A3SQL main — follows ACE3/CBA_A3 convention
#ifndef COMPONENT
    #define COMPONENT main
#endif
#ifndef COMPONENT_BEAUTIFIED
    #define COMPONENT_BEAUTIFIED A3SQL_Main
#endif

#ifndef MAINPREFIX
#define MAINPREFIX z
#endif
#ifndef PREFIX
#define PREFIX a3sql
#endif

#include "script_version.hpp"

#ifndef VERSION
#define VERSION     MAJOR.MINOR
#endif
#ifndef VERSION_STR
#define VERSION_STR MAJOR.MINOR.PATCHLVL.BUILD
#endif
#ifndef VERSION_AR
#define VERSION_AR  MAJOR,MINOR,PATCHLVL,BUILD
#endif

#ifndef REQUIRED_VERSION
#define REQUIRED_VERSION 2.02
#endif

#ifdef SUBCOMPONENT_BEAUTIFIED
    #define COMPONENT_NAME QUOTE(A3SQL - COMPONENT_BEAUTIFIED - SUBCOMPONENT_BEAUTIFIED)
#else
    #ifndef COMPONENT_NAME
    #define COMPONENT_NAME QUOTE(A3SQL - COMPONENT_BEAUTIFIED)
    #endif
#endif
