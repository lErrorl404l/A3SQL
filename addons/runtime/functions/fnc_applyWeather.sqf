#include "../script_component.hpp"

// Built-in apply function: modify weather parameters.
// Called by fnc_apply when apply_function = "a3sql_runtime_fnc_applyWeather".
// Reads wind, rain, humidity from rule value and applies via setWind, setRain, etc.
// Value format: "wind_x,wind_y,rain,humidity" (any subset, comma-separated).
// Operator: set replaces, mul scales the existing value.
params [
    ["_target", objNull, [objNull]],
    ["_rule", createHashMap, [createHashMap]],
    ["_context", createHashMap, [createHashMap]]
];

private _operator = toLower (_rule getOrDefault ["operator", "set"]);
private _value    = _rule getOrDefault ["value", ""];
private _parts    = _value splitString " ,";

// Parse optional fields (empty string = no change)
private _windX     = if (count _parts > 0) then { parseNumber (_parts select 0) } else { -1 };
private _windY     = if (count _parts > 1) then { parseNumber (_parts select 1) } else { -1 };
private _rain      = if (count _parts > 2) then { parseNumber (_parts select 2) } else { -1 };
private _humidity  = if (count _parts > 3) then { parseNumber (_parts select 3) } else { -1 };

private _results = [];

// Wind
if (_windX > -1 || _windY > -1) do {
    private _current = wind;
    private _cx = _current select 0;
    private _cy = _current select 1;
    switch (_operator) do {
        case "set": {
            if (_windX > -1) then { _cx = _windX };
            if (_windY > -1) then { _cy = _windY };
        };
        case "mul": {
            _cx = _cx * (_windX max 0);
            _cy = _cy * (_windY max 0);
        };
        case "add": {
            _cx = _cx + _windX;
            _cy = _cy + _windY;
        };
    };
    setWind [_cx, _cy, false];
    _results pushBack "wind";
};

// Rain
if (_rain > -1) do {
    private _current = rain;
    private _val = _current;
    switch (_operator) do {
        case "set": { _val = _rain; };
        case "mul": { _val = _current * _rain; };
        case "add": { _val = (_current + _rain) max 0 min 1; };
    };
    setRain (_val max 0 min 1); // lint-ignore: setRain takes Number per Arma 3 wiki
    _results pushBack "rain";
};

// Humidity (ACE3 or future mods may expose this)
if (_humidity > -1) do {
    // Arma 3 vanilla has no setHumidity; store for mods that read it
    missionNamespace setVariable ["A3SQL_weather_humidity", _humidity];
    _results pushBack "humidity";
};

if (count _results == 0) exitWith { [1, "ERR_PARAM", "No valid weather fields in value"] };

[0, "OK", _results]
