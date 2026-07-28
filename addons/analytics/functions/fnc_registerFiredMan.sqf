#include "..\script_component.hpp"

// FiredMan must be attached to specific units via addEventHandler
{
    _x addEventHandler ["FiredMan", { _this call a3sql_analytics_fnc_handleFiredMan; }];
} forEach (allPlayers - entities "HeadlessClient_F");
