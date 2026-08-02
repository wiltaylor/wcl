#!/usr/bin/env bash
# Run one heavy merge-bar command holding a machine-wide lock, so the several
# worktrees on this box queue for the machine instead of oversubscribing it.
#
#     bash .just/ci-lock.sh <command> [args...]
#
# WHY this exists, WHAT it is bolted onto and WHICH commands get it are all
# documented once, on `ci_lock` in .just/shared.just. This file is the how.
#
# Environment:
#   WCL_CI_LOCK_DISABLE=1   run unlocked (a machine with one user)
#   WCL_CI_LOCK_FILE=PATH   lock file (default ${XDG_RUNTIME_DIR:-/tmp}/wcl-ci.lock)
#   WCL_CI_LOCK_TIMEOUT=N   bounded wait, in seconds (default 1800)
#
# MISSING TOOLING DEGRADES; A REAL TIMEOUT FAILS. No `flock` on the box, or a
# lock file that won't open, warns and runs unlocked — the lock is a throughput
# optimization, and a machine that can't take it must still be able to run the
# gate. Waiting out the full WCL_CI_LOCK_TIMEOUT is the opposite case: the lock
# works, something is holding it far longer than any gate should, and exiting
# non-zero with the holder named beats blocking forever.

set -uo pipefail

note() { printf 'ci-lock: %s\n' "$*" >&2; }

# Seconds as `1m05s`, clamped at zero so a skewed clock can't print `-1m-5s`.
duration() {
    local s=$1
    [ "$s" -ge 0 ] 2>/dev/null || s=0
    printf '%dm%02ds' $(( s / 60 )) $(( s % 60 ))
}

[ "$#" -gt 0 ] || { note "usage: ci-lock.sh <command> [args...]"; exit 2; }

# Every spelling of "no" means no, so a `WCL_CI_LOCK_DISABLE=false` meant as an
# opt-OUT of the opt-out can't silently read as an opt-in.
case "$(printf '%s' "${WCL_CI_LOCK_DISABLE:-}" | tr '[:upper:]' '[:lower:]')" in
    '' | 0 | false | no | off) ;;
    *) exec "$@" ;;
esac

lock_file="${WCL_CI_LOCK_FILE:-${XDG_RUNTIME_DIR:-/tmp}/wcl-ci.lock}"
holder_file="$lock_file.holder"
timeout="${WCL_CI_LOCK_TIMEOUT:-1800}"
case "$timeout" in
    '' | *[!0-9]*)
        note "ignoring non-numeric WCL_CI_LOCK_TIMEOUT=$timeout"
        timeout=1800
        ;;
esac

# Who is in there, for the message printed BEFORE we start waiting. flock keeps
# no record of its holder, so the holder writes one beside the lock; a dead pid
# means the record outlived a killed run, so say nothing specific.
holder_desc() {
    local pid since cmd
    [ -r "$holder_file" ] || { printf 'another run'; return; }
    read -r pid since cmd <"$holder_file" 2>/dev/null || true
    case "${pid:-}" in '' | *[!0-9]*) printf 'another run'; return ;; esac
    kill -0 "$pid" 2>/dev/null || { printf 'another run'; return; }
    case "${since:-}" in
        '' | *[!0-9]*) printf 'pid %s (%s)' "$pid" "${cmd:-?}" ;;
        *) printf 'pid %s (%s), running for %s' \
            "$pid" "${cmd:-?}" "$(duration $(( $(date +%s) - since )))" ;;
    esac
}

# util-linux only. A mac or Windows dev box has no flock, and warning on every
# locked command would be a dozen identical lines per gate, so warn hourly.
if ! command -v flock >/dev/null 2>&1; then
    stamp="$lock_file.noflock"
    if [ -z "$(find "$stamp" -mmin -60 2>/dev/null)" ]; then
        note "flock not found — running unlocked; merge-bar runs on this machine are not serialized"
        : 2>/dev/null >"$stamp" || true
    fi
    exec "$@"
fi

# Probe the open as a plain command, because a failed redirection on `exec`
# below would kill this shell outright — no degrade path, no message. That is
# the whole reason the two steps are separate. (2>/dev/null comes FIRST:
# redirections apply left to right, and the shell reports a failed one on
# whatever stderr is current when it fails.)
if ! : 2>/dev/null >>"$lock_file"; then
    note "cannot open lock file $lock_file — running unlocked"
    exec "$@"
fi

# The lock file's contents are never read or written — fd 9 exists only for
# flock to hold. Append rather than truncate, since this open happens BEFORE the
# lock is taken and must not disturb the run that already holds it.
exec 9>>"$lock_file"

# Probe first so contention is reported the instant it happens. A silent block
# reads as a hang, and a caller with a timeout of its own needs to see why.
if ! flock -n 9; then
    note "waiting for the merge-bar lock ($lock_file) — held by $(holder_desc); up to $(duration "$timeout") (WCL_CI_LOCK_DISABLE=1 to skip)"
    waited_from=$(date +%s)
    if ! flock -w "$timeout" 9; then
        note "gave up after $(duration "$timeout") waiting for $lock_file — held by $(holder_desc); re-run, or set WCL_CI_LOCK_DISABLE=1"
        exit 1
    fi
    note "lock acquired after $(duration $(( $(date +%s) - waited_from )))"
fi

printf '%s %s %s\n' "$$" "$(date +%s)" "$*" 2>/dev/null >"$holder_file" || true

# fd 9 survives the exec, so the lock is held for the command's whole life and
# released by the kernel when it exits — however it exits. Descendants inherit
# it too, so a command that leaves a background process behind keeps the lock
# until that process also exits. None of the locked gate commands do; a
# long-lived server (`wdoc serve`, `wcl editor`) must never be locked.
exec "$@"
