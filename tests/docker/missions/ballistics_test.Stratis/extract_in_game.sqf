// A3SQL MSS Ballistics Demo — in-game config extraction
// Reads ballistic values DIRECTLY from the loaded config (post-CBA/ACE) and
// writes them into the a3sql database. This replaces host-side PBO extraction
// as the source of "original" values, and captures the real parent class via
// inheritsFrom (the host parser cannot see CBA/ACE merged hierarchies).
//
// Table: ballistics_extract(source, weapon, magazine, ammo, layer, property,
//        value, parent)

diag_log text "=== A3SQL IN-GAME EXTRACTION START ===";

"a3sql" callExtension "CREATE TABLE IF NOT EXISTS ballistics_extract (id INTEGER PRIMARY KEY AUTOINCREMENT, source TEXT, weapon TEXT, magazine TEXT, ammo TEXT, layer TEXT, property TEXT, value TEXT, parent TEXT)";

// Ballistic properties worth capturing (vanilla + ACE)
private _props = [
    "initSpeed", "airFriction", "typicalSpeed", "hit", "maxSpeed", "caliber",
    "ACE_caliber", "ACE_bulletLength", "ACE_bulletMass", "ACE_ballisticCoefficients",
    "ACE_velocityBoundaries", "ACE_dragModel", "ACE_muzzleVelocities",
    "ACE_barrelLengths", "ACE_standardAtmosphere", "ACE_transonicStabilityCoef",
    "ACE_ammoTempMuzzleVelocityShifts"
];

private _fnc_insert = {
    params ["_source", "_weapon", "_class", "_layer", "_prop", "_val", "_parent"];
    private _sql = format [
        "INSERT INTO ballistics_extract (source, weapon, magazine, ammo, layer, property, value, parent) VALUES ('%1', '%2', '%3', '%4', '%5', '%6', '%7', '%8')",
        _source, _weapon,
        if (_layer == "magazine") then { _class } else { "" },
        if (_layer == "ammo") then { _class } else { "" },
        _layer, _prop, _val, _parent
    ];
    "a3sql" callExtension _sql;
};

// Walk every weapon in CfgWeapons, find the ones whose classname starts with
// our demo prefixes, extract their magazine -> ammo chain.
private _fnc_extract_weapon = {
    params ["_wpn"];

    private _cfgWeapon = configFile >> "CfgWeapons" >> _wpn;
    private _mags = getArray (_cfgWeapon >> "magazines");
    if (_mags isEqualTo []) exitWith {};

    {
        private _mag = _x;
        private _cfgMag = configFile >> "CfgMagazines" >> _mag;
        if (!isClass _cfgMag) then { continue };

        private _magParent = configName (inheritsFrom _cfgMag);
        private _ammo = getText (_cfgMag >> "ammo");
        private _cfgAmmo = configFile >> "CfgAmmo" >> _ammo;
        private _ammoParent = if (isClass _cfgAmmo) then { configName (inheritsFrom _cfgAmmo) } else { "" };

        {
            private _p = _x;
            if (isNumber (_cfgMag >> _p)) then {
                [_run, _wpn, _mag, "magazine", _p, str (getNumber (_cfgMag >> _p)), _magParent] call _fnc_insert;
            };
            if (isArray (_cfgMag >> _p)) then {
                [_run, _wpn, _mag, "magazine", _p, str (getArray (_cfgMag >> _p)), _magParent] call _fnc_insert;
            };
        } forEach _props;

        if (isClass _cfgAmmo) then {
            {
                private _p = _x;
                if (isNumber (_cfgAmmo >> _p)) then {
                    [_run, _wpn, _ammo, "ammo", _p, str (getNumber (_cfgAmmo >> _p)), _ammoParent] call _fnc_insert;
                };
                if (isArray (_cfgAmmo >> _p)) then {
                    [_run, _wpn, _ammo, "ammo", _p, str (getArray (_cfgAmmo >> _p)), _ammoParent] call _fnc_insert;
                };
            } forEach _props;
        };
    } forEach _mags;
};

private _run = missionNamespace getVariable ["a3sql_ballistics_run", "extract"];

// Demo weapons — extract their full chain
private _demoWeapons = [
    "MSS_SR25_65CM_22_LMT_BLK",
    "Mss_M107A1_50_29_GRY",
    "MSS_Mk18_24_300NM_CPS_BLK"
];

{
    if (isClass (configFile >> "CfgWeapons" >> _x)) then {
        [_x] call _fnc_extract_weapon;
        diag_log text format ["[EXTRACT] %1 done", _x];
    } else {
        diag_log text format ["[EXTRACT] %1 NOT FOUND", _x];
    };
} forEach _demoWeapons;

diag_log text "=== A3SQL IN-GAME EXTRACTION COMPLETE ===";