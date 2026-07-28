#include "../script_component.hpp"

// Returns all online players with their current state
["SELECT * FROM players WHERE online=1 ORDER BY name"] call a3sql_fnc_selectMap;
