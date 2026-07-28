#include "../script_component.hpp"

params [["_table", "events_kills", [""]]];
[_table] call a3sql_database_fnc_exportCSV;
