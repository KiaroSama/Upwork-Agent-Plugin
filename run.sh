#!/bin/sh
set -eu
umask 077
root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
if [ ! -x "$root/bin/upwork-connector" ]; then
    printf '%s\n' 'Native connector is missing. Download and extract the complete release archive for this OS and architecture.' >&2
    exit 1
fi
if [ "$#" -eq 0 ]; then set -- menu; fi
exec "$root/bin/upwork-connector" --root "$root" "$@"
