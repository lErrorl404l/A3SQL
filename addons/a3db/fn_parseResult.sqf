#include "script_component.hpp"

/*
    Parse a3db extension JSON response into SQF array.
    Input:  "[0,"OK","data"]"  or  "[[header],[row1]]"
    Output: SQF array via CBA_fnc_parseJSON

    CBA is a hard dependency (CfgPatches a3db_main requires cba_main).
*/

params [["_response", "", [""]]];

if (_response isEqualTo "") exitWith { [0, "", []] };

_response call CBA_fnc_parseJSON
