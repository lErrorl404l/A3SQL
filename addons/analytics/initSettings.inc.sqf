#include "script_component.hpp"

["a3sql_analytics_sample_interval", "SLIDER",
    ["STR_A3SQL_Analytics_SampleInterval_DisplayName", "STR_A3SQL_Analytics_SampleInterval_Description"],
    "STR_A3SQL_Analytics_Category",
    [30, 300, 60, 0],
    false
] call CBA_fnc_addSetting;

["a3sql_analytics_debug", "CHECKBOX",
    ["STR_A3SQL_Analytics_Debug_DisplayName", "STR_A3SQL_Analytics_Debug_Description"],
    "STR_A3SQL_Analytics_Category", false, false
] call CBA_fnc_addSetting;

["a3sql_analytics_stream_output", "CHECKBOX",
    ["STR_A3SQL_Analytics_StreamOutput_DisplayName", "STR_A3SQL_Analytics_StreamOutput_Description"],
    "STR_A3SQL_Analytics_Category", false, false
] call CBA_fnc_addSetting;
