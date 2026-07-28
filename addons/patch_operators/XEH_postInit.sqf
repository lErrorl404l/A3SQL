#include "script_component.hpp"

// ── Apply config overrides at mission start ───────────────────────
[] call a3sql_patch_operators_fnc_applyOverrides;
