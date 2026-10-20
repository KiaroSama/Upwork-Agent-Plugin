#!/bin/sh
set -eu
umask 077
# Monitor mode gives this invocation its own verifiable child process group.
if ! (set -m 2>/dev/null; case $- in *m*) exit 0;; *) exit 1;; esac); then
    [ -z "${BASH_VERSION:-}" ] && command -v bash >/dev/null 2>&1 || { printf '%s\n' 'Build requires a shell with monitor-mode process ownership; install nothing automatically.' >&2; exit 1; }
    unset BASH_ENV ENV
    exec bash --noprofile --norc "$0" "$@"
fi
set -m
root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
command -v cargo >/dev/null 2>&1 || { printf '%s\n' 'Rust 1.99 or newer is required. No global installation is performed.' >&2; exit 1; }
mkdir -p "$root/.ci-work" "$root/.cargo-cache" "$root/.ci-cache" "$root/logs"
export TMPDIR="$root/.ci-work" TMP="$root/.ci-work" TEMP="$root/.ci-work" CARGO_BUILD_JOBS=2
export CARGO_HOME="$root/.cargo-cache" CARGO_TARGET_DIR="$root/target" XDG_CACHE_HOME="$root/.ci-cache"
log="$root/logs/upwork-build_$(date -u +%Y-%m-%d_%H-%M-%S)_UTC_$$.log"
event() {
    line=$(printf '[%s UTC] [%s] [upwork-build] %s' "$(date -u '+%Y-%m-%d %H:%M:%S')" "$1" "$2")
    if ! printf '%s\n' "$line" >> "$log"; then printf '%s\n' 'Build file logging unavailable.' "$line" >&2; fi
}
cd "$root"
owned=$(mktemp -d "$root/.ci-work/unix-build.XXXXXXXX")
child= group= staged=
cleanup() {
    code=$?
    trap - EXIT HUP INT TERM
    if [ -n "$group" ]; then
        kill -KILL -- "-$group" 2>/dev/null || :
    elif [ -n "$child" ]; then
        kill -KILL "$child" 2>/dev/null || :
    fi
    if [ -n "$child" ]; then wait "$child" 2>/dev/null || :; fi
    if [ -n "$staged" ]; then rm -f -- "$staged"; fi
    if [ "$code" -ne 0 ]; then rm -f -- "$root/.ci-work/build-provenance.json"; fi
    rm -f -- "$owned/start"
    rmdir -- "$owned"
    event INFO "Native build stopped exit_code=$code; owned process cleanup completed."
    exit "$code"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
# A receipt from a previous build must not authorize a new failed build.
rm -f -- "$root/.ci-work/build-provenance.json"
[ "$(git rev-parse --show-toplevel)" = "$root" ] || { event ERROR 'Build requires this project root to be the Git checkout root.'; exit 1; }
source=$(git rev-parse HEAD)
tree=$(git rev-parse 'HEAD^{tree}')
git diff --quiet HEAD -- || { event ERROR 'Build requires unchanged tracked inputs.'; exit 1; }
untracked=$(git ls-files --others --exclude-standard) || { event ERROR 'Cannot inspect build inputs.'; exit 1; }
[ -z "$untracked" ] || { event ERROR 'Build refuses untracked non-ignored inputs.'; exit 1; }
compiler=$(rustc -vV)
host=$(printf '%s\n' "$compiler" | awk '/^host: / {print $2}')
version=$(awk '/^version = "[0-9]+\.[0-9]+\.[0-9]+"$/ {gsub(/"/, "", $3); print $3; exit}' Cargo.toml)
case "$host" in x86_64-unknown-linux-gnu|aarch64-apple-darwin|x86_64-apple-darwin) ;; *) event ERROR 'Unsupported build target for provenance.'; exit 1;; esac
[ -n "$version" ] || { event ERROR 'Package version unavailable.'; exit 1; }
wall=${UPWORK_BUILD_WALL_SECONDS:-900}
idle=${UPWORK_BUILD_IDLE_SECONDS:-600}
case "$wall:$idle" in *[!0-9:]*|:*|*:) event ERROR 'Invalid build time limits.'; exit 1;; esac
[ "$wall" -ge 1 ] && [ "$wall" -le 900 ] && [ "$idle" -ge 1 ] && [ "$idle" -le 600 ] || { event ERROR 'Build limits must be positive and cannot exceed 900/600 seconds.'; exit 1; }
event INFO "Native build started wall_seconds=$wall idle_seconds=$idle."
# The child cannot execute Cargo until its isolated process group is verified.
# A bounded flag gate avoids blocking the owner if the child fails before startup.
sh -c 'n=0; while [ ! -f "$1" ]; do n=$((n + 1)); [ "$n" -le 5 ] || exit 1; sleep 1; done; exec cargo build --release --locked' sh "$owned/start" >> "$log" 2>&1 < /dev/null &
child=$!
pgid=$(ps -o pgid= -p "$child" | tr -d '[:space:]')
[ "$pgid" = "$child" ] || { event ERROR 'Cannot establish isolated build process ownership.'; exit 1; }
group=$pgid
event INFO "Build process ownership pid=$child pgid=$group."
printf '%s\n' start > "$owned/start"
start=$(date +%s)
last=$start
size=$(wc -c < "$log")
while kill -0 "$child" 2>/dev/null; do
    now=$(date +%s)
    current=$(wc -c < "$log")
    if [ "$current" -ne "$size" ]; then last=$now; size=$current; fi
    if [ "$((now - start))" -ge "$wall" ] || [ "$((now - last))" -ge "$idle" ]; then
        event ERROR 'Native build exceeded wall or idle bound; existing binary preserved.'
        exit 124
    fi
    # Bounded progress polling, not a readiness sleep.
    sleep 1
done
code=0
wait "$child" || code=$?
child=
# Cargo may not leave descendant jobs behind even on a nominally successful exit.
if kill -0 -- "-$group" 2>/dev/null; then
    event ERROR 'Build left descendants running; publication refused.'
    exit 1
fi
group=
[ "$code" -eq 0 ] || { event ERROR "Native build failed exit_code=$code; existing binary preserved."; exit "$code"; }
[ "$(git rev-parse HEAD)" = "$source" ] && [ "$(git rev-parse 'HEAD^{tree}')" = "$tree" ] && git diff --quiet HEAD -- || { event ERROR 'Build source changed; publication refused.'; exit 1; }
untracked=$(git ls-files --others --exclude-standard) || { event ERROR 'Cannot recheck build inputs.'; exit 1; }
[ -z "$untracked" ] || { event ERROR 'Build inputs changed; publication refused.'; exit 1; }
[ "$(rustc -vV)" = "$compiler" ] || { event ERROR 'Compiler identity changed; publication refused.'; exit 1; }
binary="$root/target/release/upwork-connector"
[ -f "$binary" ] && [ ! -L "$binary" ] || { event ERROR 'Release executable missing or linked.'; exit 1; }
if command -v sha256sum >/dev/null 2>&1; then digest=$(sha256sum "$binary" | awk '{print $1}'); else digest=$(shasum -a 256 "$binary" | awk '{print $1}'); fi
[ "${#digest}" -eq 64 ] || { event ERROR 'Release digest unavailable.'; exit 1; }
printf '{"source_sha":"%s","source_tree":"%s","target":"%s","version":"%s","binary_sha256":"%s","transformation":"unsigned-build"}\n' "$source" "$tree" "$host" "$version" "$digest" > "$root/.ci-work/build-provenance.json"
mkdir -p bin config .state
chmod 700 config .state logs
staged=$(mktemp "$root/bin/.build.XXXXXXXX")
cp "$binary" "$staged"
chmod 700 "$staged"
mv -f -- "$staged" "$root/bin/upwork-connector"
staged=
event INFO 'Native build complete; source and executable digest recorded.'
printf '%s\n' 'Native connector built. Run ./run.sh.'
