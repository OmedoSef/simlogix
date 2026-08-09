#!/usr/bin/env bash
#
# Cuts a release: sets the version, checks the tree, tags it, and pushes —
# which is what the Release workflow watches for.
#
#   scripts/release.sh 0.2.0
#   scripts/release.sh 0.2.0 --dry-run     say what would happen, change nothing
#   scripts/release.sh 0.2.0 --no-push     commit and tag here, push by hand
#   scripts/release.sh 0.2.0 --no-verify   skip the checks (they are the point)
#
# Run it inside the devcontainer: the checks need cargo, and so does the
# commit hook.
#
# The version lives once, in [workspace.package]. Both crates read it from
# there, and the application shows it through CARGO_PKG_VERSION — so setting
# it here is what changes what the About window says.

set -euo pipefail

die() { printf '\033[31merror:\033[0m %s\n' "$*" >&2; exit 1; }
step() { printf '\033[1m==>\033[0m %s\n' "$*"; }

version=""
dry_run=false
push=true
verify=true

for arg in "$@"; do
    case "$arg" in
        --dry-run)   dry_run=true ;;
        --no-push)   push=false ;;
        --no-verify) verify=false ;;
        -h|--help)   sed -n '2,16p' "$0" | sed 's|^# \{0,1\}||'; exit 0 ;;
        -*)          die "unknown option: $arg" ;;
        *)           [ -z "$version" ] || die "give one version, not two"
                     version="$arg" ;;
    esac
done

[ -n "$version" ] || die "usage: $0 <version> [--dry-run] [--no-push] [--no-verify]"

# A leading v is what the tag carries, not what the manifest holds. Accept
# either spelling and keep the two straight from here on.
version="${version#v}"
tag="v$version"

# Deliberately strict: a version is written into a tag that cannot be moved
# once anyone has fetched it.
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] \
    || die "'$version' is not a semantic version (1.2.3, or 1.2.3-rc.1)"

cd "$(dirname "$0")/.."
[ -f Cargo.toml ] || die "no Cargo.toml — this should be the repository root"

# ---------------------------------------------------------------------------
# Refuse rather than surprise
# ---------------------------------------------------------------------------

branch="$(git rev-parse --abbrev-ref HEAD)"
[ "$branch" = "main" ] || die "on '$branch' — releases are cut from main"

[ -z "$(git status --porcelain)" ] \
    || die "the working tree has changes; commit or stash them first"

git rev-parse -q --verify "refs/tags/$tag" >/dev/null \
    && die "$tag already exists here"

if git ls-remote --exit-code --tags origin "$tag" >/dev/null 2>&1; then
    die "$tag already exists on origin — a published tag is not one to move"
fi

current="$(git show HEAD:Cargo.toml | sed -n '/^\[workspace\.package\]/,/^\[/p' \
    | sed -n 's/^version = "\(.*\)"/\1/p' | head -1)"
[ -n "$current" ] || die "could not read the current version from Cargo.toml"
[ "$current" != "$version" ] || die "the version is already $version"

step "$current → $version, tagged $tag"

# ---------------------------------------------------------------------------
# Set the version
# ---------------------------------------------------------------------------

set_version() {
    # Only the `version` inside [workspace.package]: every other `version =`
    # in this file belongs to a dependency.
    awk -v new="$1" '
        /^\[/ { in_section = ($0 == "[workspace.package]") }
        in_section && !done && /^version = / { print "version = \"" new "\""; done = 1; next }
        { print }
    ' Cargo.toml > Cargo.toml.new
    mv Cargo.toml.new Cargo.toml
}

if [ "$dry_run" = true ]; then
    step "dry run — stopping before anything is written"
    printf '  would set   [workspace.package] version = "%s"\n' "$version"
    printf '  would run   cargo fmt --check, clippy, test\n'
    printf '  would commit chore: release %s\n' "$version"
    printf '  would tag    %s\n' "$tag"
    [ "$push" = true ] && printf '  would push   main and %s to origin\n' "$tag"
    exit 0
fi

# Anything below can leave the tree edited, so put it back on the way out
# unless we got all the way to the commit.
committed=false
cleanup() {
    if [ "$committed" = false ]; then
        printf '\033[33mputting Cargo.toml and Cargo.lock back\033[0m\n' >&2
        git checkout -- Cargo.toml Cargo.lock 2>/dev/null || true
    fi
}
trap cleanup EXIT

set_version "$version"

# Rewrites the lockfile's entries for our own crates. Without this the commit
# would carry a Cargo.toml and a Cargo.lock disagreeing about the version,
# and the first build afterwards would quietly fix it.
step "refreshing Cargo.lock"
cargo metadata --format-version 1 --offline >/dev/null 2>&1 \
    || cargo metadata --format-version 1 >/dev/null

# ---------------------------------------------------------------------------
# Check before tagging, not after
# ---------------------------------------------------------------------------

if [ "$verify" = true ]; then
    step "checking (the same things CI will)"
    cargo fmt --all -- --check
    cargo clippy --all-targets --all-features -- -D warnings
    cargo test --all-features

    # A release whose notices are out of date is a release shipping the wrong
    # attribution. Comparing costs one run of a tool that is already built.
    step "checking the third-party notices are current"
    tmp="$(mktemp -d)"
    cargo run -q -p simlogix-gui --bin write-licenses -- \
        "$tmp/THIRD-PARTY.md" "$tmp/third-party.json" >/dev/null
    if ! cmp -s "$tmp/THIRD-PARTY.md" THIRD-PARTY.md; then
        rm -rf "$tmp"
        die "THIRD-PARTY.md is out of date — regenerate it and commit that first:
  cargo run -p simlogix-gui --bin write-licenses -- THIRD-PARTY.md assets/third-party.json"
    fi
    rm -rf "$tmp"
else
    step "skipping the checks — CI will run them, after the tag is spent"
fi

# ---------------------------------------------------------------------------
# Commit, tag, push
# ---------------------------------------------------------------------------

printf '\n'
git --no-pager diff --stat Cargo.toml Cargo.lock
printf '\n'
if [ "$push" = true ]; then
    printf 'This will commit, tag %s, and push both to origin.\n' "$tag"
    printf 'Pushing the tag starts the release workflow and publishes it.\n'
else
    printf 'This will commit and tag %s here. Nothing will be pushed.\n' "$tag"
fi
printf 'Continue? [y/N] '
read -r answer
case "$answer" in
    y|Y|yes) ;;
    *) die "stopped" ;;
esac

git add Cargo.toml Cargo.lock
git commit -m "chore: release $version"
committed=true

# Annotated, so the tag carries a date and an author of its own — a release
# is a thing that happened, not just a name for a commit.
git tag -a "$tag" -m "SimLogix $version"

if [ "$push" = true ]; then
    step "pushing"
    git push origin main
    git push origin "$tag"
    printf '\n\033[32mdone\033[0m — the Release workflow is building %s.\n' "$tag"
    remote="$(git remote get-url origin)"
    case "$remote" in
        git@github.com:*) remote="https://github.com/${remote#git@github.com:}" ;;
    esac
    printf '  %s/actions\n' "${remote%.git}"
else
    printf '\n\033[32mdone\033[0m — nothing pushed. When you are ready:\n'
    printf '  git push origin main && git push origin %s\n' "$tag"
fi
