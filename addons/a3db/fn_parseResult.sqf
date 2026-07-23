#include "script_component.hpp"

/*
    Parse a3db extension JSON response into SQF array.
    Input:  "[0,"OK","data"]"  or  "[[header],[row1]]"
    Output: [returnCode, status, data]  or  [[header], [row1], ...]
*/

params [["_response", "", [""]]];

if (_response isEqualTo "") exitWith { [0, "", []] };

// Try parsing as JSON
private _parsed = _response call CBA_fnc_parseJSON;

// If CBA isn't loaded or the result is still a string, do basic split
if (_parsed isEqualType "") exitWith {
    // Minimal fallback parser
    private _parts = _parsed splitString ",";
    _parts
};

_parsed
