#include "script_component.hpp"

["a3sql_analytics_sample_interval", "SLIDER",
    ["Sample Interval (s)", "How often to sample server performance metrics."],
    "A3SQL Analytics",
    [30, 300, 60, 0],
    false
] call CBA_fnc_addSetting;

["a3sql_analytics_debug", "CHECKBOX",
    ["Debug", "Enable verbose logging."],
    "A3SQL Analytics", false, false
] call CBA_fnc_addSetting;

["a3sql_analytics_stream_output", "CHECKBOX",
    ["Stream Output", "Show analytics notifications via systemChat."],
    "A3SQL Analytics", false, false
] call CBA_fnc_addSetting;
