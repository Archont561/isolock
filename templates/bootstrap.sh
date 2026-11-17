#!/bin/sh
# Restore a named toolchain environment from a local Isolock CAS or isolock/v1.
# Source the generated .isolock/state/<environment>/activate to use the tools.

set -eu
LC_ALL=C
export LC_ALL
IFS=$(printf ' \t')
umask 077

usage() {
    printf '%s\n' \
        'Usage: sh .isolock/bootstrap.sh [--remote] [environment]' \
        '       sh .isolock/bootstrap.sh --help' \
        '' \
        'Restore an environment (default: active) from .isolock/blobs and' \
        '.isolock/environments/<environment>.manifest. With --remote, fetch' \
        'isolock/v1 and read its top-level blobs/ and environments/ directories.'
}

fail() {
    printf 'isolock bootstrap: %s\n' "$*" >&2
    exit 1
}

remote=0
while [ "$#" -gt 0 ]; do
    case "$1" in
        --remote)
            remote=1
            shift
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        --)
            shift
            break
            ;;
        -* )
            fail "unknown option: $1 (try --help)"
            ;;
        *)
            break
            ;;
    esac
done

if [ "$#" -gt 1 ]; then
    usage >&2
    exit 2
fi
environment=${ISOLOCK_ENV:-active}
if [ "$#" -eq 1 ]; then
    environment=$1
fi
case "$environment" in
    ''|.|..|*[!A-Za-z0-9._-]*)
        fail "invalid environment name: $environment"
        ;;
esac

script_dir=$(dirname "$0")
case "$script_dir" in
    /*) ;;
    *) script_dir=$PWD/$script_dir ;;
esac
if ! project_root=$(CDPATH='' cd -P "$script_dir/.." && pwd -P); then
    fail 'cannot locate the project root from the bootstrap script'
fi
isolock_dir=$project_root/.isolock
state_root=$isolock_dir/state
work_dir=$state_root/.bootstrap-work-$environment-$$
staging_dir=$state_root/.bootstrap-stage-$environment-$$
backup_dir=$state_root/.bootstrap-backup-$environment-$$
environment_dir=$state_root/$environment

cleanup() {
    cleanup_status=$?
    if [ -d "$backup_dir" ]; then
        if [ ! -d "$environment_dir" ]; then
            if ! mv "$backup_dir" "$environment_dir"; then
                printf 'isolock bootstrap: cannot restore previous environment from %s\n' "$backup_dir" >&2
                cleanup_status=1
            fi
        elif ! rm -rf "$backup_dir"; then
            printf 'isolock bootstrap: cannot remove temporary backup %s\n' "$backup_dir" >&2
            cleanup_status=1
        fi
    fi
    rm -rf "$work_dir" "$staging_dir" || cleanup_status=1
    return "$cleanup_status"
}

mkdir -p "$state_root"
mkdir "$work_dir" "$staging_dir"
trap cleanup 0
trap 'exit 1' HUP INT TERM

command -v tar >/dev/null 2>&1 || fail 'tar is required'
if ! command -v sha256sum >/dev/null 2>&1 && ! command -v shasum >/dev/null 2>&1; then
    fail 'sha256sum (or shasum -a 256) is required'
fi

if [ "$remote" -eq 1 ]; then
    command -v git >/dev/null 2>&1 || fail 'git is required for --remote'
    storage_ref=refs/remotes/origin/isolock/v1
    if ! git -C "$project_root" fetch --depth=1 origin "refs/heads/isolock/v1:$storage_ref"; then
        fail 'cannot fetch remote storage branch isolock/v1'
    fi
    if [ -z "$(git -C "$project_root" ls-tree "$storage_ref" blobs)" ]; then
        fail 'isolock/v1 does not contain a top-level blobs/ directory'
    fi
    if [ -z "$(git -C "$project_root" ls-tree "$storage_ref" environments)" ]; then
        fail 'isolock/v1 does not contain a top-level environments/ directory'
    fi
    storage_archive=$work_dir/isolock-v1.tar
    if ! git -C "$project_root" archive --format=tar "$storage_ref" blobs environments > "$storage_archive"; then
        fail 'cannot archive storage from isolock/v1'
    fi
    mkdir -p "$isolock_dir"
    if ! (CDPATH='' cd -P "$isolock_dir" && tar -xf "$storage_archive"); then
        fail 'cannot unpack storage from isolock/v1'
    fi
fi

manifest=$isolock_dir/environments/$environment.manifest
if [ ! -f "$manifest" ] && [ -f "$project_root/environments/$environment.manifest" ]; then
    manifest=$project_root/environments/$environment.manifest
fi
[ -f "$manifest" ] || fail "environment manifest not found: $manifest"

blob_root=$isolock_dir/blobs
if [ ! -d "$blob_root" ] && [ -d "$project_root/blobs" ]; then
    blob_root=$project_root/blobs
fi
[ -d "$blob_root" ] || fail "blob store not found: $blob_root"

verify_sha256() {
    expected_digest=$1
    artifact_file=$2
    if command -v sha256sum >/dev/null 2>&1; then
        digest_output=$(sha256sum < "$artifact_file") || fail "cannot hash $artifact_file"
    else
        digest_output=$(shasum -a 256 < "$artifact_file") || fail "cannot hash $artifact_file"
    fi
    actual_digest=${digest_output%% *}
    [ "$actual_digest" = "$expected_digest" ] ||
        fail "SHA-256 mismatch for $artifact_file: expected $expected_digest, found $actual_digest"
}

shell_quote() {
    printf "'"
    printf '%s' "$1" | sed "s/'/'\\\\''/g"
    printf "'"
}

printf 'Restoring environment %s from %s\n' "$environment" "$manifest"
manifest_entries=0
while read -r digest filename extra || [ -n "${digest:-}${filename:-}${extra:-}" ]; do
    case "${digest:-}" in
        ''|\#*)
            continue
            ;;
    esac
    [ -z "${extra:-}" ] || fail 'manifest entries must contain exactly a digest and an archive filename'
    case "$digest" in
        *[!0123456789abcdef]*)
            fail "invalid lowercase SHA-256 digest in manifest: $digest"
            ;;
    esac
    [ "${#digest}" -eq 64 ] || fail "invalid SHA-256 digest length in manifest: $digest"
    case "${filename:-}" in
        ''|.|..|*[!A-Za-z0-9._+-]*)
            fail "invalid archive filename in manifest: ${filename:-<empty>}"
            ;;
    esac

    first_two=$(printf '%.2s' "$digest")
    remainder=${digest#??}
    second_two=$(printf '%.2s' "$remainder")
    blob_file=$blob_root/sha256/$first_two/$second_two/$digest/$filename
    archive_file=$blob_file
    if [ ! -f "$blob_file" ]; then
        first_part=$blob_file.part000
        [ -f "$first_part" ] || fail "CAS artifact not found: $blob_file (or its .part* shards)"
        archive_file=$work_dir/$filename
        : > "$archive_file"
        part_count=0
        for part_file in "$blob_file".part*; do
            if [ -f "$part_file" ]; then
                cat "$part_file" >> "$archive_file"
                part_count=$((part_count + 1))
            fi
        done
        [ "$part_count" -gt 0 ] || fail "no CAS shards found for $blob_file"
        printf 'Reassembled %s from %s shard(s)\n' "$filename" "$part_count"
    fi

    verify_sha256 "$digest" "$archive_file"
    if ! (CDPATH='' cd -P "$staging_dir" && tar -xf "$archive_file"); then
        fail "cannot unpack verified archive $filename"
    fi
    manifest_entries=$((manifest_entries + 1))
    printf 'Verified and unpacked %s\n' "$filename"
done < "$manifest"

[ "$manifest_entries" -gt 0 ] || fail "environment manifest contains no artifacts: $manifest"
[ -d "$staging_dir/bin" ] || fail 'archives must provide a top-level bin/ directory'

{
    printf '%s\n' '# Generated by Isolock; source this file to activate the environment.'
    printf 'PATH='
    shell_quote "$environment_dir/bin"
    printf "\${PATH:+:\$PATH}\nexport PATH\n"
} > "$staging_dir/activate"

if [ -d "$environment_dir" ]; then
    mv "$environment_dir" "$backup_dir"
fi
if ! mv "$staging_dir" "$environment_dir"; then
    fail "cannot install restored environment at $environment_dir"
fi
if [ -d "$backup_dir" ]; then
    rm -rf "$backup_dir"
fi

printf 'Environment ready. Activate it with: . .isolock/state/%s/activate\n' "$environment"
