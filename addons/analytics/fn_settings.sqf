#include "script_component.hpp"

["a3sql_analytics_sample_interval", "SLIDER",
    [["STR_a3sql_analytics_setting_sample_interval", "STR_a3sql_analytics_setting_sample_interval_desc"]],
    "A3SQL Analytics",
    [30, 300, 60, 0],
    false
] call CBA_fnc_addSetting;

["a3sql_analytics_debug", "CHECKBOX",
    [["STR_a3sql_analytics_setting_debug", "STR_a3sql_analytics_setting_debug_desc"]],
    "A3SQL Analytics", false, false
] call CBA_fnc_addSetting;

["a3sql_analytics_stream_output", "CHECKBOX",
    [["STR_a3sql_analytics_setting_stream_output", "STR_a3sql_analytics_setting_stream_output_desc"]],
    "A3SQL Analytics", false, false
] call CBA_fnc_addSetting;
