#include "..\script_component.hpp"

addMissionEventHandler ["FiredMan", { _this call a3sql_analytics_fnc_handleFiredMan; }];
