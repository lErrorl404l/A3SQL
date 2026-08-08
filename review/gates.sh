#!/usr/bin/env bash
# Gate harness for the a3db/a3sql rules-compliance review.
# Falsifiable, machine-checkable gates against the pinned snapshot.
#
# Usage: bash review/gates.sh <GATE>     e.g. bash review/gates.sh G0

set -u

PINNED_SHA="585a46079d7b64e4cdf3d1c9a859d710602a91b9"
SNAPSHOT="review/00-snapshot.md"

fail() { echo "GATE FAIL: $1" >&2; exit 1; }
pass() { echo "GATE PASS: $1"; }

g0_snapshot() {
    # The review branch builds ON the pin; strict equality would never pass
    # after the snapshot commit lands. Correct semantics: pin is an ancestor.
    git merge-base --is-ancestor "$PINNED_SHA" HEAD \
        || fail "pinned $PINNED_SHA is not an ancestor of HEAD"
    pass "HEAD descends from pinned $PINNED_SHA"

    [ -f "$SNAPSHOT" ] || fail "snapshot $SNAPSHOT missing"
    grep -q "SHA: $PINNED_SHA" "$SNAPSHOT" \
        || fail "snapshot does not pin SHA: $PINNED_SHA"
    pass "snapshot $SNAPSHOT pins SHA $PINNED_SHA"
}

g1_triage() {
    # Clause-level triage (review/01-triage.md) vs the briefs corpus.
    # Corpus = briefs/*.md in the rules config dir (briefs-index.md, 7 Aug 2026).
    local file="review/01-triage.md"
    [ -f "$file" ] || fail "triage $file missing"

    local briefs_dir="$HOME/.config/opencode/rules/briefs"
    [ -d "$briefs_dir" ] || fail "briefs corpus dir missing: $briefs_dir"

    # 1) corpus inventory: every *.md in briefs/ is a brief
    local -a corpus=()
    local b
    for b in "$briefs_dir"/*.md; do
        corpus+=("$(basename "$b" .md)")
    done
    [ "${#corpus[@]}" -eq 34 ] \
        || fail "corpus count ${#corpus[@]} != 34 (index drift)"

    # 2) parse triage rows: | # | brief | STATUS | reason | ... |
    local rows app=0 na=0
    rows=$(grep -cE '^\| *[0-9]+ *\|' "$file")
    [ "$rows" -eq 34 ] || fail "triage has $rows rows, expected 34"

    local -A seen=()
    local line brief status reason
    while IFS= read -r line; do
        [[ "$line" =~ ^\|[[:space:]]*[0-9]+[[:space:]]*\| ]] || continue
        brief=$(awk -F'|' '{gsub(/^ +| +$/, "", $3); print $3}' <<<"$line")
        status=$(awk -F'|' '{gsub(/^ +| +$/, "", $4); print $4}' <<<"$line")
        reason=$(awk -F'|' '{gsub(/^ +| +$/, "", $5); print $5}' <<<"$line")
        [ -n "$brief" ] || fail "row missing brief name"
        [ "$status" = "APPLIES" ] || [ "$status" = "N/A" ] \
            || fail "row '$brief' status '$status' not APPLIES/N/A"
        [ -n "$reason" ] || fail "row '$brief' missing reason"
        [ -z "${seen[$brief]:-}" ] || fail "brief '$brief' listed more than once"
        seen[$brief]=1
        if [ "$status" = "APPLIES" ]; then app=$((app+1)); else na=$((na+1)); fi
    done < "$file"

    # 3) every corpus brief present exactly once
    local missing=0
    for b in "${corpus[@]}"; do
        if [ -z "${seen[$b]:-}" ]; then
            echo "missing from triage: $b" >&2
            missing=$((missing+1))
        fi
    done
    [ "$missing" -eq 0 ] || fail "$missing corpus brief(s) missing from triage"

    # 4) counts match the clerk matrix: 11 APPLIES / 23 N/A
    [ "$app" -eq 11 ] || fail "APPLIES count $app != 11"
    [ "$na" -eq 23 ] || fail "N/A count $na != 23"
    pass "triage: 34/34 briefs covered, $app APPLIES / $na N/A, rows valid"
}

g2_adjudication() {
    # G2 adjudication: every finding record in 02-evidence-*.md and
    # 03-adjudication.md satisfies the 7-field template:
    #   F-### | artifact (path+line+pin) | harm | fix |
    #   exactly one of {brief-clause, gap-rider} | status+owner+date | version
    # Plus: <=18 findings, F-01 exists and is HIGH, >=1 gap-rider,
    # zero duplicate F-IDs in the final list.
    local adjud="review/03-adjudication.md"
    local files=(
        review/02-evidence-e1-security.md
        review/02-evidence-e2-provenance.md
        review/02-evidence-e3-licence.md
        review/02-evidence-e4-writing.md
    )
    local f
    for f in "${files[@]}"; do
        [ -f "$f" ] || fail "evidence file $f missing"
    done
    [ -f "$adjud" ] || fail "adjudication $adjud missing"

    # ---- 1. per-finding 7-field template on evidence files (awk) ----
    # Sections are delimited by '## F-NN' headers. Each section is joined to
    # one line, then tested for the template markers. Any violation prints
    # 'VIOLATION <file>:<line> <id>: <reason>'.
    local vout=""
    for f in "${files[@]}"; do
        vout+="$(awk -v file="$f" -v pin="585a460" '
        function bad(reason) {
            print "VIOLATION " file ":" start " " id ": " reason
            rc = 1
        }
        function evaluate() {
            # a) artifact: pin SHA + a resolvable reference
            if (blob !~ pin) { bad("artifact missing pin " pin) }
            if (blob !~ /:[0-9]/ && blob !~ /git log/ && blob !~ /grep/ \
                && blob !~ /\.(rs|toml|md|yml|h|sh|lock|py)/) {
                bad("artifact has no path:line or command reference")
            }
            # c) harm: harm label or severity token
            if (blob !~ /\bharm\b/ && blob !~ /(HIGH|MED|LOW)(-MED)?/) {
                bad("harm field missing")
            }
            # c) fix: fix word
            if (blob !~ /fix/i) { bad("fix field missing") }
            # d) exactly one of {brief-clause, gap-rider}
            gapped  = (blob ~ /clause:[[:space:]]*gap-rider/ \
                       || blob ~ /\|[[:space:]]*clause[[:space:]]*\|[[:space:]]*gap-rider/)
            briefed = (blob ~ /clause:[[:space:]]*(ncsc-|uk-|defence-|jsp-|gds|gov\.uk|mil-std|aqap|dodi|osint)/ \
                       || blob ~ /\|[[:space:]]*clause[[:space:]]*\|[[:space:]]*(ncsc-|uk-|defence-|jsp-|gds|gov\.uk|mil-std|aqap|dodi|osint)/ \
                       || blob ~ /\|[[:space:]]*`?(writers-handbook|security-classifications)`?[[:space:]]*:/)
            if (gapped == briefed) {
                bad("must cite exactly one of {brief-clause, gap-rider}")
            }
            # e) status + owner + date
            if (blob !~ /(OPEN|FIXED|WAIVED)/) { bad("status missing") }
            if (blob !~ /(lead|auditor|clerk|architect|researcher)/) { bad("owner missing") }
            if (blob !~ /20[0-9]{2}-[0-9]{2}-[0-9]{2}/) { bad("date missing") }
        }
        BEGIN { rc = 0; section = 0 }
        /^##[[:space:]]+F-[0-9]+/ {
            if (section) evaluate()          # previous section, before id changes
            id = $2
            if (id !~ /^F-[0-9]+$/) { print "VIOLATION " file ":" NR ": bad section header: " $0; rc = 1 }
            start = NR
            blob = ""
            section = 1
            next
        }
        section { blob = blob " " $0 }
        END {
            if (section) evaluate()
            exit rc
        }' "$f")"
    done
    if [ -n "$vout" ]; then
        printf '%s\n' "$vout" >&2
        fail "G2: evidence template violations (above)"
    fi

    # ---- 2. every cross-file duplicate ID must be resolved in 03 ----
    # The collision mapping table in 03-adjudication.md names each duplicated ID.
    local mapblock dups d
    mapblock=$(awk '/^##[[:space:]]+Collision mapping/{m=1;next} /^##[[:space:]]+/ && m {exit} m{print}' "$adjud")
    dups=$(grep -hoE '^##[[:space:]]+F-[0-9]+' "${files[@]}" \
        | awk '{print $2}' | sort | uniq -d)
    for d in $dups; do
        printf '%s\n' "$mapblock" | grep -q "$d" \
            || fail "duplicate evidence ID $d not resolved in $adjud collision mapping"
    done

    # ---- 3. final list in 03: 7-field rows, <=18, F-01 HIGH, >=1 gap-rider ----
    local final
    final=$(awk -F'|' '
    function bad(id, reason) { print "VIOLATION 03-adjudication.md: " id ": " reason; rc = 1 }
    BEGIN { rc = 0; n = 0; f01high = 0; gaps = 0 }
    {
        if ($0 !~ /^\| F-[0-9]+ \|/) next
        id=$2; sev=$3; art=$4; harm=$5; fix=$6; cls=$7; st=$8
        gsub(/^[[:space:]]+|[[:space:]]+$/, "", id)
        gsub(/^[[:space:]]+|[[:space:]]+$/, "", cls)
        gsub(/^[[:space:]]+|[[:space:]]+$/, "", sev)
        if ($0 !~ /^\| F-[0-9]+ \| (HIGH|MED|LOW|LOW-MED) \|/) { bad("", "bad final row: " $0); next }
        if (cls !~ /^brief-clause/ && cls !~ /^gap-rider/) { bad(id, "classification field must be brief-clause or gap-rider"); next }
        if (art == "" || harm == "" || fix == "") { bad(id, "artifact/harm/fix empty in final list"); next }
        if (st !~ /(OPEN|FIXED|WAIVED)/ || st !~ /20[0-9]{2}-[0-9]{2}-[0-9]{2}/) { bad(id, "status+owner+date malformed"); next }
        if (seen[id]++) { bad(id, "duplicate F-ID in final list") }
        n++
        if (id == "F-01" && sev == "HIGH") f01high = 1
        if (cls ~ /^gap-rider/) gaps = 1
    }
    END {
        if (n > 18) { print "VIOLATION 03-adjudication.md: final list has " n " findings (>18)"; rc = 1 }
        if (!f01high) { print "VIOLATION 03-adjudication.md: F-01 must exist and be HIGH"; rc = 1 }
        if (!gaps) { print "VIOLATION 03-adjudication.md: no gap-rider finding in final list"; rc = 1 }
        print "G2 SUMMARY: " n " findings, F-01 HIGH: " (f01high ? "yes" : "no") \
            ", gap-riders: " (gaps ? "yes" : "no")
        exit rc
    }' "$adjud")
    printf '%s\n' "$final" | grep '^G2 SUMMARY' || true
    if printf '%s\n' "$final" | grep -q '^VIOLATION'; then
        printf '%s\n' "$final" >&2
        fail "G2: final-list violations (above)"
    fi
    pass "adjudication: all finding records pass the 7-field template; final list valid"
}

g3_findings_report() {
    # G3 closure: findings report issued as the jsp-945 configuration item.
    # (a) report exists, version line == v1.1; (b) CM baseline SHA pinned;
    # (c) OFFICIAL classification note present; (d) every findings-table row
    # has status in {OPEN, FIXED, WAIVED} + owner + date; (e) exactly 14
    # findings rows; (f) zero duplicate F-IDs; (g) F-01 present and HIGH.
    local file="review/04-findings-report.md"
    [ -f "$file" ] || fail "findings report $file missing"

    grep -qE '^Version: v1\.1|^- Version: v1\.1' "$file" \
        || fail "report version line is not v1.1"
    pass "report version v1.1"

    grep -q "baseline: $PINNED_SHA" "$file" \
        || fail "report does not pin CM baseline $PINNED_SHA"
    pass "report pins CM baseline $PINNED_SHA"

    grep -q 'OFFICIAL' "$file" \
        || fail "report carries no OFFICIAL classification note"
    pass "report marked OFFICIAL"

    # Parse only the findings table (## 2. Findings table), delimited by its
    # '| ID |' header; later tables keep their own headers so do not match.
    local out
    out=$(awk -F'|' '
    function bad(id, reason) { print "VIOLATION " id ": " reason; rc = 1 }
    BEGIN { rc = 0; in_table = 0; n = 0; f01high = 0 }
    /^##[[:space:]]/ { in_table = 0 }
    /^\| ID \|/ { in_table = 1; next }
    in_table && /^\| F-[0-9]+ \|/ {
        id=$2; sev=$3; cls=$7; st=$8; ver=$9
        gsub(/^[[:space:]]+|[[:space:]]+$/, "", id)
        gsub(/^[[:space:]]+|[[:space:]]+$/, "", sev)
        gsub(/^[[:space:]]+|[[:space:]]+$/, "", cls)
        gsub(/^[[:space:]]+|[[:space:]]+$/, "", st)
        gsub(/^[[:space:]]+|[[:space:]]+$/, "", ver)
        if (id !~ /^F-[0-9]+$/) { bad(id, "bad finding id"); next }
        if (sev !~ /^(HIGH|MED|LOW|LOW-MED)$/) { bad(id, "severity not HIGH/MED/LOW/LOW-MED: '" sev "'"); next }
        if (cls !~ /^brief-clause/ && cls !~ /^gap-rider/) { bad(id, "classification must be brief-clause or gap-rider"); next }
        if (st !~ /^(OPEN|FIXED|WAIVED|no-action)/) { bad(id, "status not OPEN/FIXED/WAIVED/no-action: '" st "'"); next }
        if (st !~ /(lead|auditor|clerk|architect|researcher)/) { bad(id, "owner missing"); next }
        if (st !~ /20[0-9]{2}-[0-9]{2}-[0-9]{2}/) { bad(id, "date missing"); next }
        if (ver != "v1.1") { bad(id, "version not v1.1: '" ver "'"); next }
        if (seen[id]++) { bad(id, "duplicate F-ID") }
        n++
        if (id == "F-01" && sev == "HIGH") f01high = 1
    }
    END {
        if (n != 14) { print "VIOLATION findings table has " n " rows, expected 14"; rc = 1 }
        if (!f01high) { print "VIOLATION F-01 must be present and HIGH"; rc = 1 }
        print "G3 SUMMARY: " n " findings, F-01 HIGH: " (f01high ? "yes" : "no")
        exit rc
    }' "$file")
    printf '%s\n' "$out" | grep '^G3 SUMMARY' || true
    if printf '%s\n' "$out" | grep -q '^VIOLATION'; then
        printf '%s\n' "$out" >&2
        fail "G3: findings-table violations (above)"
    fi
    pass "findings report: 14 findings, no duplicate IDs, F-01 HIGH, statuses valid"
}

case "${1:-}" in
    G0) g0_snapshot ;;
    G1) g1_triage ;;
    G2) g2_adjudication ;;
    G3) g3_findings_report ;;
    *)  echo "usage: $0 G0|G1|G2|G3" >&2; exit 2 ;;
esac
