#!/bin/sh
set -eu
umask 077
launcher_log() {
    case "$1" in INFO|WARNING|ERROR|DEBUG) ;; *) return 2 ;; esac
    printf '[%s] [%s] [launcher] %s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" "$1" "$2" >&2
}
root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
if [ ! -x "$root/bin/upwork-connector" ]; then
    launcher_log ERROR 'Native connector is missing. Extract the complete release archive for this OS and architecture.'
    exit 1
fi
# Cache validation and creation belong to native startup, after command validation.
export TEMP="$root/.cache/tmp" TMP="$root/.cache/tmp" TMPDIR="$root/.cache/tmp" XDG_CACHE_HOME="$root/.cache"
if [ "$#" -eq 0 ]; then set -- menu; fi
launcher_log INFO 'Starting the native connector.'
launcher_log DEBUG 'Native startup owns cache validation and directory creation.'
exec "$root/bin/upwork-connector" --root "$root" "$@"
