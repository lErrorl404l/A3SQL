#include "../script_component.hpp"

"DELETE FROM server_commands WHERE status='executed'" call a3sql_fnc_execute;

[0, "OK", "Executed commands cleared"]
