#include "script_component.hpp"

["a3sql_analytics_sample_interval", "SLIDER",
    [LSTRING(SampleInterval_DisplayName), LSTRING(SampleInterval_Description)],
    LSTRING(Category),
    [30, 300, 60, 0],
    false
] call CBA_fnc_addSetting;

["a3sql_analytics_debug", "CHECKBOX",
    [LSTRING(Debug_DisplayName), LSTRING(Debug_Description)],
    LSTRING(Category), false, false
] call CBA_fnc_addSetting;

["a3sql_analytics_stream_output", "CHECKBOX",
    [LSTRING(StreamOutput_DisplayName), LSTRING(StreamOutput_Description)],
    LSTRING(Category), false, false
] call CBA_fnc_addSetting;
