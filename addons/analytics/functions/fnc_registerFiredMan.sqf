#include "..\script_component.hpp"

private _event = "FiredMan";
addMissionEventHandler [_event, { _this call a3sql_analytics_fnc_handleFiredMan; }];
