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

case "${1:-}" in
    G0) g0_snapshot ;;
    *)  echo "usage: $0 G0" >&2; exit 2 ;;
esac
