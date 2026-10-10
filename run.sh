#!/bin/sh
set -eu
umask 077
root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
if [ ! -x "$root/bin/upwork-connector" ]; then
    printf '%s\n' 'Native connector is missing. Download and extract the complete release archive for this OS and architecture.' >&2
    exit 1
fi
mkdir -p "$root/.cache/tmp"
export TEMP="$root/.cache/tmp" TMP="$root/.cache/tmp" TMPDIR="$root/.cache/tmp" XDG_CACHE_HOME="$root/.cache"
if [ "$#" -eq 0 ]; then set -- menu; fi
exec "$root/bin/upwork-connector" --root "$root" "$@"
