#include "script_component.hpp"

["SELECT * FROM patch_rules WHERE match_type='init' ORDER BY priority"] call a3sql_fnc_selectMap
