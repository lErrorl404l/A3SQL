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

case "${1:-}" in
    G0) g0_snapshot ;;
    G1) g1_triage ;;
    *)  echo "usage: $0 G0|G1" >&2; exit 2 ;;
esac
