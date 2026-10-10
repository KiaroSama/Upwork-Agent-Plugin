#!/bin/sh
set -eu
umask 077
root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
if [ ! -x "$root/bin/upwork-connector" ]; then
    printf '%s\n' 'Native connector is missing. Run build.sh first; no global installation is performed.' >&2
    exit 1
fi
if [ "$#" -eq 0 ]; then set -- menu; fi
exec "$root/bin/upwork-connector" --root "$root" "$@"
