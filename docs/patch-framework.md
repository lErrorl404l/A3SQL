# A3SQL Patch Framework

Live modification of in-game values without writing config.cpp compatibility patches.

---

- [Overview](#overview)
- [Quick Start](#quick-start)
- [Architecture](#architecture)
- [SQF API Reference](#sqf-api-reference)
  - [Core Functions](#core-functions)
  - [Value Transformer Operators](#value-transformer-operators)
  - [Utility Handlers](#utility-handlers)
- [SQL Schema](#sql-schema)
- [Target Collections (target_type)](#target-collections-target_type)
- [Match Types (match_type)](#match-types-match_type)
- [Operators](#operators)
- [CBA Settings](#cba-settings)
- [Custom Handlers](#custom-handlers)
- [Worked Example](#worked-example-patching-a-vehicles-fuel)
- [Best Practices](#best-practices)
- [Security Notes](#security-notes)

---

## Overview

Patch rules are stored in an SQL database (`patch_rules` table) and applied at
runtime by SQF code. A CBA PerFrame handler periodically checks for new rules
via a missionNamespace dirty-flag. When new rules are detected,
`fn_applyAll` reads them in batches (LIMIT 50 pagination), iterates each
wrapped in `try {} catch {}`, and applies the patch via target collection
scoping, match-type filtering, and operator transforms.

No mission restart, no config.cpp edits, no PBO rebuild. Write a row into
`patch_rules` and the patch system applies it on the next frame.

---

## Quick Start

Load any Arma 3 mission with a3sql loaded, open the debug console or
use init.sqf, and run:

```sqf
"a3sql" callExtension ["live_patch", ["texture", "myObject", "path\to\texture.paa"]];
```

The extension creates a `patch_rules` row with `target_type='texture'`,
`property='myObject'`, `value='path\to\texture.paa'`. The PerFrame handler
picks it up on the next cycle and applies the texture.

Or insert a rule directly:

```sqf
private _sql = "INSERT INTO patch_rules (name, active, match_type, match_value, target_type, property, operator, value) VALUES ('btr_fuel', 1, 'exact', 'B_MBT_01_cannon_F', 'vehicle', 'fuel', 'mul', '1.5')";
"a3sql" callExtension _sql;
[] call a3sql_patch_fnc_reload;
```

---

## Architecture

SQL stores all patch rules in the `patch_rules` table. A CBA PerFrame handler
periodically checks for new rules via a missionNamespace dirty-flag
(`a3sql_patch_dirty`). When new rules are detected, `fn_applyAll` reads them
in batches (LIMIT 50 pagination), iterates each wrapped in `try {} catch {}`,
and applies the patch via target collection scoping, match-type filtering, and
operator transforms.

The flow:

1. **init.sqf / postInit** -- `fn_postInit` creates `patch_rules` via
   `CREATE TABLE IF NOT EXISTS`, starts the PerFrame handler, and registers a
   JIP handler that re-applies patches on player connect.
2. **PerFrame tick** -- Checks `a3sql_patch_dirty`. If dirty (or after 60s
   timeout), calls `fn_applyAll`.
3. **fn_applyAll** -- SELECTs active rules ordered by priority DESC, 50 at a
   time, wraps each in `try {} catch {}`, calls `fn_applyRule`.
4. **fn_applyRule** -- Resolves target collection by `target_type`, filters by
   `match_type`/`match_value`, then applies the `operator` to each matched
   object via `setVariable`.
5. **Operators** -- `set`, `inc`, `sub`, `mul`, `div`, `mod`, `cat`,
   `default`, `toggle`, `call`, `sqf_exec`, `add` (eventHandler), `remove`
   (eventHandler).

---

## SQF API Reference

### Core Functions

#### a3sql_patch_fnc_applyAll

Apply all active rules from the database.

```
Parameters:
  _extension  STRING  (default: "a3sql")  Extension name

Return:  ARRAY  [0, "OK", [appliedCount, errorCount]]
```

Reads rules in batches of 50 (paginated via LIMIT/OFFSET), ordered by
priority DESC then id ASC. Each rule is wrapped in `try {} catch {}` so one
failing rule does not stop the batch.

```sqf
_result = [] call a3sql_patch_fnc_applyAll;
// [0, "OK", [12, 1]]  -- 12 applied, 1 error
```

#### a3sql_patch_fnc_applyRule

Apply a single rule from a hashmap or DB row array.

```
Parameters:
  _rule       ARRAY / HASHMAP  Rule data (from getRule or DB row)
  _extension  STRING  (default: "a3sql")

Return:  ARRAY  [0, "OK", [appliedCount, failedCount]]
         or     [1, errorCode, errorMessage]
```

Accepts either a hashmap with named keys or a positional array matching the
`patch_rules` column order. Handles match_value arrays (JSON-like strings
starting with `[` and ending with `]` are parsed via `parseSimpleArray`).

```sqf
// From hashmap
_result = [_ruleHashmap] call a3sql_patch_fnc_applyRule;

// From raw DB row
_result = [dbRowArray] call a3sql_patch_fnc_applyRule;
```

#### a3sql_patch_fnc_applyByTarget

Apply all rules matching a specific target_type + match_value combo.

```
Parameters:
  _targetType  STRING   Target type to filter by
  _matchValue  STRING   Match value to filter by
  _extension   STRING   (default: "a3sql")

Return:  ARRAY  [0, "OK", [totalApplied, totalErrors]]
         or     [1, errorCode, errorMessage]
```

Queries `patch_rules WHERE active = 1 AND target_type = X AND match_value = Y`
and applies each matching rule.

```sqf
_result = ["vehicle", "B_MBT_01_cannon_F"] call a3sql_patch_fnc_applyByTarget;
```

#### a3sql_patch_fnc_getRule

Fetch a single rule by its database ID.

```
Parameters:
  _ruleId     INTEGER  Rule ID (must be > 0)
  _extension  STRING   (default: "a3sql")

Return:  ARRAY  [0, "OK", hashmap]
         or     [1, errorCode, errorMessage]
```

Returns the rule as a hashmap with column names as keys. `match_value` is
automatically parsed into an array if it looks like an SQF array string.

```sqf
_result = [3] call a3sql_patch_fnc_getRule;
// [0, "OK", createHashMapFromArray [
//   ["id", 3], ["name", "btr_fuel"], ["active", 1],
//   ["target_type", "vehicle"], ...
// ]]
```

#### a3sql_patch_fnc_listRules

List all rules, optionally filtered.

```
Parameters:
  _filter     STRING / HASHMAP  (default: "")  Name search or hashmap filter
  _extension  STRING  (default: "a3sql")

Return:  ARRAY  Raw SQL response (parseSimpleArray result)
```

Two filtering modes:

- **String filter**: `LIKE '%value%'` search on rule name.
- **Hashmap filter**: Key-value equality match on any column.

```sqf
// List all rules
_rules = [] call a3sql_patch_fnc_listRules;

// Search by name
_rules = ["btr"] call a3sql_patch_fnc_listRules;

// Filter by hashmap
_rules = [createHashMapFromArray [
    ["target_type", "vehicle"], ["active", 1]
]] call a3sql_patch_fnc_listRules;
```

#### a3sql_patch_fnc_deleteRule

Delete a rule by ID.

```
Parameters:
  _ruleId     INTEGER  Rule ID to delete (must be > 0)
  _extension  STRING   (default: "a3sql")

Return:  ARRAY  Raw SQL response from DELETE
```

Sets the dirty flag on success so the PerFrame handler picks up the change.

```sqf
_result = [3] call a3sql_patch_fnc_deleteRule;
```

#### a3sql_patch_fnc_setDirty

Set or clear the dirty flag that triggers rule re-application.

```
Parameters:
  _dirty      BOOL    (default: true)
  _extension  STRING  (default: "a3sql")

Return:  ARRAY  [0, "OK", dirtyValue]
```

```sqf
// Mark rules as needing re-apply
[] call a3sql_patch_fnc_setDirty;

// Clear dirty flag
[false] call a3sql_patch_fnc_setDirty;
```

#### a3sql_patch_fnc_reload

Force a full re-apply of all rules immediately.

```
Parameters:
  _extension  STRING  (default: "a3sql")

Return:  ARRAY  [0, "OK", "Reload queued"]
```

Sets the dirty flag and calls `fn_applyAll` immediately if the system is
enabled.

```sqf
[] call a3sql_patch_fnc_reload;
```

#### a3sql_patch_fnc_registerHandler

Register a custom target type handler function.

```
Parameters:
  _handlerName  STRING   Name for the handler (e.g. "myType")
  _code         CODE     Handler function
  _extension    STRING   (default: "a3sql")

Return:  ARRAY  [0, "OK", variableName]
```

The handler is stored as `missionNamespace getVariable
["a3sql_patch_handler_<name>", ...]`. When applyRule encounters a target_type
that matches a registered handler, it calls the handler with
`[_matchValue, _property, _value]`.

```sqf
["myCustomType", {
    params ["_matchValue", "_property", "_value"];
    // Custom target finding and modification
    {
        _x setVariable [_property, _value];
    } forEach (allMissionObjects "All" select { typeOf _x == _matchValue });
}] call a3sql_patch_fnc_registerHandler;
```

### Value Transformer Operators

These are the operator backend functions. They are called internally by
`fn_applyRule` for the `inc`, `sub`, `mul`, `div`, `mod`, `cat`, and
`default` operators. They can also be used directly.

#### a3sql_patch_fnc_opAdd

Numeric addition of two values.

```
Parameters:
  _value  ANY  (default: 0)  Base value (parsed as number)
  _param  ANY  (default: 0)  Value to add (parsed as number)

Return:  NUMBER   _value + _param
```

```sqf
_result = [5, "3"] call a3sql_patch_fnc_opAdd;  // 8
```

#### a3sql_patch_fnc_opSub

Numeric subtraction.

```
Parameters:
  _value  ANY  (default: 0)  Base value
  _param  ANY  (default: 0)  Value to subtract

Return:  NUMBER   _value - _param
```

```sqf
_result = [10, "3"] call a3sql_patch_fnc_opSub;  // 7
```

#### a3sql_patch_fnc_opMul

Numeric multiplication.

```
Parameters:
  _value  ANY  (default: 0)  Base value
  _param  ANY  (default: 0)  Multiplier

Return:  NUMBER   _value * _param
```

```sqf
_result = [4, "1.5"] call a3sql_patch_fnc_opMul;  // 6
```

#### a3sql_patch_fnc_opDiv

Numeric division. Returns 0 on division by zero.

```
Parameters:
  _value  ANY  (default: 0)  Dividend
  _param  ANY  (default: 0)  Divisor

Return:  NUMBER   _value / _param, or 0 if _param == 0
```

```sqf
_result = [10, "2"] call a3sql_patch_fnc_opDiv;  // 5
_result = [10, "0"] call a3sql_patch_fnc_opDiv;  // 0
```

#### a3sql_patch_fnc_opMod

Numeric modulo. Returns 0 on modulo by zero.

```
Parameters:
  _value  ANY  (default: 0)  Dividend
  _param  ANY  (default: 0)  Divisor

Return:  NUMBER   _value % _param, or 0 if _param == 0
```

```sqf
_result = [10, "3"] call a3sql_patch_fnc_opMod;  // 1
```

#### a3sql_patch_fnc_opCat

String concatenation.

```
Parameters:
  _value  ANY  (default: "")  Base value (stringified)
  _param  ANY  (default: "")  Appended value (stringified)

Return:  STRING   str(_value) + str(_param)
```

```sqf
_result = ["hello", " world"] call a3sql_patch_fnc_opCat;  // "hello world"
```

#### a3sql_patch_fnc_opDefault

Return the default value if the current value is nil, empty, zero, or false.

```
Parameters:
  _value    ANY  (default: "")  Current value (can be nil)
  _default  ANY  (default: "")  Fallback default

Return:  ANY   _default if _value is nil/""/0/false, else _value
```

```sqf
_result = [nil, "default"] call a3sql_patch_fnc_opDefault;  // "default"
_result = ["existing", "default"] call a3sql_patch_fnc_opDefault;  // "existing"
```

### Utility Handlers

These are standalone handler functions designed for use with
`registerHandler` or direct calling. They are **not** wired into the default
rule dispatch pipeline (which works via target collection + setVariable).
Register them as custom handlers or call them directly from your own code.

#### a3sql_patch_fnc_handleWeapon

Set weapon ammunition by classname.

```
Parameters:
  _matchValue  STRING   Weapon classname to match
  _property    STRING   (unused, pass "")
  _value       STRING   Ammo amount (parsed as number)

Return:  ARRAY   List of affected units/vehicles
```

Scans allUnits + vehicles. Checks primaryWeapon, secondaryWeapon, and
handgunWeapon on each. Calls `setVehicleAmmo` on matches.

```sqf
_affected = ["LMG_Mk200_F", "", "200"] call a3sql_patch_fnc_handleWeapon;
```

Note: Weapon config properties (reloadTime, dispersion, etc.) are read-only
at runtime. This function only controls ammo count.

#### a3sql_patch_fnc_handleVehicle

Set vehicle properties by classname.

```
Parameters:
  _matchValue  STRING   Vehicle classname
  _property    STRING   Property name ("fuel", "damage", "ammo", or custom)
  _value       STRING   Value to set

Return:  ARRAY   List of affected vehicles
```

Filters `vehicles` by `typeOf`. Known properties use native commands:
- `fuel` -> `setFuel (parseNumber _value)`
- `damage` -> `setDamage (parseNumber _value)`
- `ammo` -> `setVehicleAmmo 1`
- anything else -> `setVariable [_property, _value]`

```sqf
_affected = ["B_MBT_01_cannon_F", "fuel", "0.5"] call a3sql_patch_fnc_handleVehicle;
```

#### a3sql_patch_fnc_handleMagazine

Swap a magazine type on units and vehicles.

```
Parameters:
  _matchValue  STRING   Magazine classname to remove
  _property    STRING   (unused, pass "")
  _value       STRING   Magazine classname to add

Return:  ARRAY   List of affected units/vehicles
```

Scans allUnits + vehicles. If `_matchValue` is in `magazines _x`, removes all
instances and adds one `_value` magazine.

```sqf
_affected = ["30Rnd_556x45_Stanag", "", "30Rnd_556x45_Stanag_Tracer_Red"] call a3sql_patch_fnc_handleMagazine;
```

#### a3sql_patch_fnc_handleUnit

Set a unit skill property by classname.

```
Parameters:
  _matchValue  STRING   Unit classname
  _property    STRING   Skill property name (e.g. "aimingAccuracy", "spotDistance")
  _value       STRING   Skill value (parsed as number)

Return:  ARRAY   List of affected units
```

Filters `allUnits` by `typeOf`. Calls `setSkill [_property, parseNumber _value]`.

```sqf
_affected = ["B_soldier_AR_F", "aimingAccuracy", "0.8"] call a3sql_patch_fnc_handleUnit;
```

#### a3sql_patch_fnc_handleTexture

Apply a texture to objects by classname.

```
Parameters:
  _matchValue  STRING   Object classname
  _property    STRING   (unused, pass "")
  _value       STRING   Texture path

Return:  ARRAY   List of affected objects
```

Filters `allMissionObjects "All"` by `typeOf`. Calls
`setObjectTexture [0, _value]` on matches.

```sqf
_affected = ["Land_WoodenBox_F", "", "a3\data_f\default_texture.paa"] call a3sql_patch_fnc_handleTexture;
```

#### a3sql_patch_fnc_handleMaterial

Apply a material to objects by classname.

```
Parameters:
  _matchValue  STRING   Object classname
  _property    STRING   (unused, pass "")
  _value       STRING   Material path

Return:  ARRAY   List of affected objects
```

Filters `allMissionObjects "All"` by `typeOf`. Calls
`setObjectMaterial [0, _value]` on matches.

```sqf
_affected = ["Land_WoodenBox_F", "", "a3\data_f\default.rvmat"] call a3sql_patch_fnc_handleMaterial;
```

#### a3sql_patch_fnc_handleEntity

Set a variable on all matching objects (generic fallback).

```
Parameters:
  _matchValue  STRING   Object classname
  _property    STRING   Variable name
  _value       STRING   Variable value

Return:  ARRAY   List of affected objects
```

Filters `allMissionObjects "All"` by `typeOf`. Calls
`setVariable [_property, _value]` on matches.

```sqf
_affected = ["Land_HelipadSquare_F", "myCustomVar", "hello"] call a3sql_patch_fnc_handleEntity;
```

---

## SQL Schema

### patch_rules

| Column | Type | Default | Description |
|--------|------|---------|-------------|
| id | INTEGER | auto | Primary key, auto-increment |
| name | TEXT | required | Rule name (unique per mission context) |
| active | INTEGER | 1 | 1 = enabled, 0 = disabled |
| priority | INTEGER | 0 | Higher values are applied first |
| match_type | TEXT | 'exact' | Matching method: 'all', 'exact', 'type_of', 'wildcard', 'regex' |
| match_value | TEXT | '' | Classname, pattern, or SQF array of classnames |
| target_type | TEXT | required | Target scope: 'all', 'object', 'vehicle', 'man', 'unit', 'group', or custom |
| property | TEXT | required | Variable name or SQF command parameter |
| operator | TEXT | 'set' | Operation: 'set', 'inc', 'sub', 'mul', 'div', 'mod', 'cat', 'default', 'toggle', 'call', 'sqf_exec', 'add', 'remove' |
| value | TEXT | required | Value to apply (string, number, or SQF code) |
| created_at | TEXT | '' | Creation timestamp (Unix epoch seconds as string) |

```sql
CREATE TABLE IF NOT EXISTS patch_rules (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    active INTEGER DEFAULT 1,
    priority INTEGER DEFAULT 0,
    match_type TEXT NOT NULL DEFAULT 'exact',
    match_value TEXT DEFAULT '',
    target_type TEXT NOT NULL,
    property TEXT NOT NULL,
    operator TEXT DEFAULT 'set',
    value TEXT NOT NULL,
    created_at TEXT DEFAULT ''
);
```

`match_value` supports SQF array syntax. If the value starts with `[` and
ends with `]`, it is parsed via `parseSimpleArray` and treated as a list of
classnames. All match types accept both a single string and an array of
strings.

---

## Target Collections (target_type)

The `target_type` column selects which collection of objects to search. This
is the initial candidate set before `match_type` filtering is applied.

| target_type | Collection | Notes |
|-------------|-----------|-------|
| `all` | `allMissionObjects "All"` | Every object in the mission |
| `object` | `allMissionObjects "All"` | Same as `all` |
| `vehicle` | `vehicles` | Only vehicle objects |
| `man` | `allUnits` | Only infantry units |
| `unit` | `allUnits` | Same as `man` |
| `group` | `allGroups` | Group objects (apply to groups themselves, not individual units) |
| *(anything else)* | `allMissionObjects "All"` | Falls back to all objects |

The collection is filtered by `match_type` + `match_value` to produce the
final target list. Then the `operator` is applied to each target via
`setVariable` (or the operator-specific behavior).

---

## Match Types (match_type)

Controls how `match_value` is compared against `typeOf` each candidate target.

| match_type | Behavior | match_value format |
|------------|----------|-------------------|
| `all` | No filtering, all targets in the collection match | ignored |
| `exact` | `typeOf _x == match_value` (or `in` if array) | Single classname string, or SQF array `["class1","class2"]` |
| `type_of` | `_x isKindOf match_value` (inheritance check) | Base class or array of classes |
| `wildcard` | `CBA_fnc_matchesWildcard(typeOf _x, pattern)` | Glob pattern like `"B_*"` |
| `regex` | `CBA_fnc_matchesRegex(typeOf _x, pattern)` | Regex pattern like `"^B_"` |
| *(anything else)* | No filtering, all targets match | ignored |

When `match_value` is an SQF array string (e.g. `["B_Soldier_F","B_Officer_F"]`),
it is parsed via `parseSimpleArray` and matching uses `in` (exact) or iteration
(type_of) for element-wise comparison.

---

## Operators

The `operator` column determines what happens to each matched target.

| Operator | Behavior | Default value applied |
|----------|----------|----------------------|
| **set** | `target setVariable [property, value]` | Direct assignment |
| **inc** | Read current, add value (via opAdd) | `parseNumber current + parseNumber value` |
| **sub** | Read current, subtract value (via opSub) | `parseNumber current - parseNumber value` |
| **mul** | Read current, multiply by value (via opMul) | `parseNumber current * parseNumber value` |
| **div** | Read current, divide by value (via opDiv) | `parseNumber current / parseNumber value`, returns 0 on div-by-zero |
| **mod** | Read current, modulo value (via opMod) | `parseNumber current % parseNumber value`, returns 0 on mod-by-zero |
| **cat** | Read current, concatenate value (via opCat) | `str current + str value` |
| **default** | Keep current if truthy, else set to value (via opDefault) | Returns value if current is nil/""/0/false |
| **toggle** | `target setVariable [property, !current]` | Flips boolean |
| **call** | `missionNamespace getVariable property` called with `[target, value]` | Calls a registered function |
| **sqf_exec** | Compiles value as SQF code, calls with `[target]` | Arbitrary SQF, blocked by `a3sql_patch_allow_sqf_exec` setting |
| **add** | `target addEventHandler [property, compile value]` | Adds event handler, tracks ID in `a3sql_patch_ehRegistry` variable |
| **remove** | `target removeEventHandler [property, handlerId]` | Removes event handler by tracked ID |

### sqf_exec Security

The `sqf_exec` operator is **disabled by default**. It compiles the `value`
column as SQF code and executes it on each matched target. To enable it,
set the `a3sql_patch_allow_sqf_exec` CBA setting to true.

```sqf
// WARNING: Only enable if you trust all mission admins
a3sql_patch_allow_sqf_exec = true;
```

sqf_exec code receives the target object as `_this`. The value is capped at
1000 characters.

```sql
-- Example: Delete a vehicle via sqf_exec
INSERT INTO patch_rules (name, active, target_type, match_type, match_value, property, operator, value)
VALUES ('delete_btr', 1, 'vehicle', 'exact', 'B_MBT_01_cannon_F', '', 'sqf_exec', 'deleteVehicle _this');
```

---

## CBA Settings

Settings are registered under the **A3SQL Patch** category in CBA settings.

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `a3sql_patch_enabled` | CHECKBOX | true | Enable or disable the entire patch system |
| `a3sql_patch_log_level` | LIST | 2 (INFO) | Verbosity: 0=ERROR, 1=WARN, 2=INFO, 3=DEBUG |
| `a3sql_patch_check_interval_hz` | SLIDER | 5 | PerFrame handler check rate in Hz. 0 = every frame |
| `a3sql_patch_allow_sqf_exec` | CHECKBOX | false | Enable the `sqf_exec` operator (RCE risk) |

Log levels:
- **0 (ERROR)**: Only critical failures.
- **1 (WARN)**: Rule application errors.
- **2 (INFO)**: Normal operation messages, rule counts.
- **3 (DEBUG)**: Handler registration details, dirty flag changes.

---

## Custom Handlers

You can extend the patch system with custom target types beyond the built-in
collection scopes. Register a handler function, then use your custom type name
as `target_type` in any rule.

### Registration

```sqf
["myType", {
    params ["_matchValue", "_property", "_value"];
    // Find and modify targets
    {
        _x setVariable [_property, _value];
    } forEach (allMissionObjects "All" select { typeOf _x == _matchValue });
}] call a3sql_patch_fnc_registerHandler;
```

### Usage

Once registered, rules with `target_type = "myType"` will call your handler
with `[_matchValue, _property, _value]` from the rule row.

```sql
INSERT INTO patch_rules (name, target_type, match_value, property, operator, value)
VALUES ('custom_rule', 'myType', 'SomeObject', 'someVar', 'set', 'hello');
```

The handler runs outside the built-in target collection and match-type
filtering pipeline. Your handler is responsible for all target selection,
filtering, and modification.

### Using Built-in Handlers

The built-in handler functions (handleWeapon, handleVehicle, handleMagazine,
handleUnit, handleTexture, handleMaterial, handleEntity) can be registered as
custom handlers if you prefer their approach over the built-in pipeline:

```sqf
["weapon", a3sql_patch_fnc_handleWeapon] call a3sql_patch_fnc_registerHandler;
["vehicle", a3sql_patch_fnc_handleVehicle] call a3sql_patch_fnc_registerHandler;
["unit", a3sql_patch_fnc_handleUnit] call a3sql_patch_fnc_registerHandler;
["magazine", a3sql_patch_fnc_handleMagazine] call a3sql_patch_fnc_registerHandler;
["texture", a3sql_patch_fnc_handleTexture] call a3sql_patch_fnc_registerHandler;
["material", a3sql_patch_fnc_handleMaterial] call a3sql_patch_fnc_registerHandler;
["entity", a3sql_patch_fnc_handleEntity] call a3sql_patch_fnc_registerHandler;
```

---

## Worked Example: Patching a Vehicle's Fuel

Goal: Give the B_MBT_01_cannon_F (M2A1 Slammer) 50% more fuel capacity.

### Step 1: Insert the rule

Via SQL directly:

```sqf
private _sql = "INSERT INTO patch_rules (name, active, match_type, match_value, target_type, property, operator, value) VALUES ('mbt_fuel_boost', 1, 'exact', 'B_MBT_01_cannon_F', 'vehicle', 'fuel', 'mul', '1.5')";
"a3sql" callExtension _sql;
```

Or via `live_patch`:

```sqf
"a3sql" callExtension ["live_patch", ["vehicle", "fuel", "1.5"]];
// Creates a rule with match_value='' and operator='set' -- not the same.
// For mul operator, use direct SQL insert.
```

### Step 2: Trigger application

```sqf
[] call a3sql_patch_fnc_reload;
```

The PerFrame handler picks up the dirty flag and applies the rule. Every
vehicle of type `B_MBT_01_cannon_F` gets its `fuel` variable multiplied by
1.5.

### Step 3: Verify

```sqf
_result = [] call a3sql_patch_fnc_listRules;
// Check that mbt_fuel_boost is active
```

### Step 4: Remove the rule

```sqf
// Find the rule ID first
_rule = (["mbt_fuel_boost"] call a3sql_patch_fnc_listRules);

// Delete it
[1] call a3sql_patch_fnc_deleteRule;  // assuming id=1
```

---

## Best Practices

- **Use descriptive rule names** -- they are the only identifier besides the
  numeric ID. The `live_patch` command auto-generates names like
  `live_patch_1712345678`.
- **Set priority for ordering** -- higher priority rules apply first. Later
  rules can overwrite earlier ones if they target the same property.
- **Disable rules instead of deleting** -- set `active = 0` to keep the rule
  for later without applying it.
- **Batch inserts** -- add all rules before triggering a reload to avoid
  multiple full passes.
- **Use try/catch** -- if calling handlers directly from your own code, wrap
  in `try {} catch {}` like the framework does.
- **match_value arrays** -- for targeting multiple classnames, use SQF array
  syntax: `["class1","class2","class3"]`. The framework parses it automatically.
- **Use `type_of` match** for inheritance-based targeting instead of listing
  every subclass. `type_of: "Tank"` matches all tank variants.

---

## Security Notes

- **sqf_exec is dangerous**. The `sqf_exec` operator compiles arbitrary SQF
  from the database. If an attacker can write to `patch_rules`, they can run
  any SQF code on every server that loads those rules. Keep
  `a3sql_patch_allow_sqf_exec` disabled unless you fully trust every
  mission admin with database write access.
- **SQL injection via patch values**. The patch framework does not sanitize
  rule values beyond what the extension's parameterized query system
  provides. If you programmatically insert rules from untrusted input, use
  parameterized queries via the extension's `$1`, `$2` syntax.
- **Handler code registration**. Custom handlers registered with
  `registerHandler` are stored in `missionNamespace`. Any script can
  overwrite them. If you register handlers in a public mission, namespace
  them to avoid collisions.
- **JIP re-application**. Patches are re-applied when a player JIPs. This is
  intentional for visual patches (textures, materials) but can cause
  side-effects if your handler is not idempotent (e.g. stacking event
  handlers or incrementing values).
