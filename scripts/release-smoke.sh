#!/bin/sh
# Smoke-test the standalone binary after packaging and extracting it like a release user.
set -eu

if [ "$#" -ne 1 ]; then
    echo "usage: release-smoke.sh PATH_TO_ISOLOCK_BINARY" >&2
    exit 2
fi

binary=$1
case $binary in
    /*) ;;
    *) binary=$PWD/$binary ;;
esac
if command -v cygpath >/dev/null 2>&1; then
    binary=$(cygpath -u "$binary")
fi
if [ ! -f "$binary" ]; then
    echo "release binary does not exist: $binary" >&2
    exit 1
fi

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
expected_version=$(awk '
    /^\[workspace\.package\]$/ { in_package = 1; next }
    /^\[/ { in_package = 0 }
    in_package && /^version[[:space:]]*=/ {
        gsub(/^[^\"]*\"|\".*$/, "")
        print
        exit
    }
' "$root/Cargo.toml")
[ -n "$expected_version" ] || {
    echo "cannot determine workspace package version" >&2
    exit 1
}

scratch=$(mktemp -d "${TMPDIR:-/tmp}/isolock-release-smoke.XXXXXX")
trap 'rm -rf "$scratch"' EXIT HUP INT TERM
mkdir -p "$scratch/home" "$scratch/config"

version=$("$binary" --version)
[ "$version" = "isolock $expected_version" ] || {
    echo "unexpected release version: $version (expected isolock $expected_version)" >&2
    exit 1
}
"$binary" --help >"$scratch/help.txt"
grep -q 'Hermetic dependency airlock manager' "$scratch/help.txt"

# Exercise typed configuration and a representative clean CI plan without project state.
printf '{"artifacts":[]}' >"$scratch/plan.json"
(
    cd "$scratch"
    HOME="$scratch/home" \
        XDG_CONFIG_HOME="$scratch/config" \
        ISOLOCK_USER_CONFIG="$scratch/no-user-config.toml" \
        "$binary" config check
    HOME="$scratch/home" \
        XDG_CONFIG_HOME="$scratch/config" \
        ISOLOCK_USER_CONFIG="$scratch/no-user-config.toml" \
        ISOLOCK_PLAN="$scratch/plan.json" \
        ISOLOCK_CAS_DIR="$scratch/cas" \
        "$binary" plan --ci >"$scratch/plan-output.txt"
)
grep -q 'Verdict: ALL DEPENDENCIES SATISFIED' "$scratch/plan-output.txt"
printf 'release smoke passed: %s\n' "$version"
