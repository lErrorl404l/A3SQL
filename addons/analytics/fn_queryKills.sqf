#include "script_component.hpp"

params [["_mode", "weapons", [""]]];

switch (toLower _mode) do {
    case "weapons": {
        "SELECT killer_weapon, COUNT(*) AS kills FROM events_kills GROUP BY killer_weapon ORDER BY kills DESC LIMIT 10" call a3sql_fnc_selectMap
    };
    case "players": {
        "SELECT killer_uid, COUNT(*) AS kills FROM events_kills GROUP BY killer_uid ORDER BY kills DESC LIMIT 10" call a3sql_fnc_selectMap
    };
    case "headshots": {
        "SELECT killer_uid, COUNT(*) AS headshots FROM events_kills WHERE headshot=1 GROUP BY killer_uid ORDER BY headshots DESC LIMIT 10" call a3sql_fnc_selectMap
    };
    default {
        []
    };
};
