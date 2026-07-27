// supportInfo dumper — runs on mission start, writes engine command metadata
// to the RPT log and a file for extraction.
//
// This SQF is loaded by the auto-init mission PBO built in the Docker
// entrypoint. It captures the output of supportInfo "" and logs it so the
// extraction script can find it in the RPT.

// Wait until the mission environment is initialized
waitUntil { time > 0 };

// Give engine a moment to fully initialize
sleep 1;

// ── Capture supportInfo ──────────────────────────────────────────────────
private _info = supportInfo "";

// Log every line to RPT (the extraction script greps for n:/u:/b: lines)
{
    private _line = _x;
    diag_log text _line;
} forEach (_info splitString toString[10,13]);

// Also log the raw string as a single block (for alternative extraction)
diag_log text ("=== SUPPORTINFO_BLOCK_START ===");
diag_log text _info;
diag_log text ("=== SUPPORTINFO_BLOCK_END ===");

// Try to write to file (may be sandboxed, but worth trying)
private _success = false;
try {
    private _fh = openWrite "support_info_dump.txt";
    if (!isNil "_fh") then {
        _fh write _info;
        close _fh;
        _success = true;
    };
} catch {};

// Signal completion via RPT so the extraction script knows we're done
diag_log text "=== A3SQL_SUPPORT_DUMP_COMPLETE ===";

// Keep the mission alive briefly for the RPT to flush
sleep 5;

// If possible, trigger mission ending to free resources
private _missionEnd = createTrigger ["EmptyDetector", [0,0,0], false];
_missionEnd setTriggerActivation ["ALPHA", "PRESENT", true];
_missionEnd setTriggerStatements ["true", "endMission 'END1';", ""];
