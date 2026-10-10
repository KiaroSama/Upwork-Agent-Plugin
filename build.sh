#!/bin/sh
set -eu
umask 077
root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
command -v cargo >/dev/null 2>&1 || { printf '%s\n' 'Rust 1.99 or newer is required. No global installation is performed.' >&2; exit 1; }
mkdir -p "$root/.ci-work" "$root/logs"
export TMPDIR="$root/.ci-work" CARGO_BUILD_JOBS=2
log="$root/logs/upwork-build_$(date -u +%Y-%m-%d_%H-%M-%S)_UTC_$$.log"
printf '[%s UTC] [INFO] [upwork-build] Native build started.\n' "$(date -u '+%Y-%m-%d %H:%M:%S')" > "$log"
cd "$root"
if cargo build --release --locked; then
    mkdir -p bin config .state
    chmod 700 config .state logs
    cp target/release/upwork-connector bin/upwork-connector
    chmod 700 bin/upwork-connector
    printf '[%s UTC] [INFO] [upwork-build] Native build complete.\n' "$(date -u '+%Y-%m-%d %H:%M:%S')" >> "$log"
    printf '%s\n' 'Native connector built. Run ./run.sh.'
else
    code=$?
    printf '[%s UTC] [ERROR] [upwork-build] Native build failed exit_code=%s.\n' "$(date -u '+%Y-%m-%d %H:%M:%S')" "$code" >> "$log"
    exit "$code"
fi
