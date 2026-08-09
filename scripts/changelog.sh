#!/usr/bin/env bash
#
# Writes release notes from the commit subjects between two points.
#
#   scripts/changelog.sh v0.2.2 v0.2.3     between two tags
#   scripts/changelog.sh v0.2.3            from the tag before it
#   scripts/changelog.sh                   from the last tag to HEAD
#
# Only the subject line is used — the first line of each commit message,
# with its Conventional-Commits prefix stripped. The body is where the
# reasoning lives, and release notes are not the place to re-read it; anyone
# who wants it has the Full Changelog link at the end.
#
# Housekeeping is left out. `chore` and `ci` describe work on the project
# rather than changes to the thing being released, and a list padded with
# them is one nobody finishes reading.

set -euo pipefail

cd "$(dirname "$0")/.."

to="${2:-${1:-HEAD}}"
if [ $# -ge 2 ]; then
    from="$1"
elif from="$(git describe --tags --abbrev=0 "$to^" 2>/dev/null)"; then
    :
else
    # No earlier tag: the first release, so everything counts.
    from="$(git rev-list --max-parents=0 HEAD | tail -1)"
fi

# Sections in the order they are printed. A type not listed here falls into
# "Other changes" — better an unfamiliar prefix showing up in the wrong
# place than one silently dropped.
section_of() {
    case "$1" in
        feat)                   echo "Features" ;;
        fix)                    echo "Fixes" ;;
        perf)                   echo "Performance" ;;
        docs)                   echo "Documentation" ;;
        chore|ci)               echo "" ;;
        *)                      echo "Other changes" ;;
    esac
}

breaking=""
features=""
fixes=""
performance=""
documentation=""
other=""

# `--no-merges`: a merge commit's subject describes the merge, not a change.
# `--reverse`: oldest first, so a section reads in the order things happened.
while IFS= read -r subject; do
    [ -n "$subject" ] || continue

    if [[ "$subject" =~ ^([a-z]+)(\([^\)]*\))?(!?):\ (.*)$ ]]; then
        type="${BASH_REMATCH[1]}"
        bang="${BASH_REMATCH[3]}"
        text="${BASH_REMATCH[4]}"
    else
        # Not a Conventional Commit. Kept whole rather than guessed at.
        type="other"
        bang=""
        text="$subject"
    fi

    section="$(section_of "$type")"
    [ -n "$section" ] || continue

    line="- $text"
    # A breaking change is the one thing a reader must not miss, so it goes
    # to the top whatever its type — and stays in its own section too, since
    # it is still a feature or a fix.
    if [ "$bang" = "!" ]; then
        breaking+="$line"$'\n'
    fi

    case "$section" in
        Features)      features+="$line"$'\n' ;;
        Fixes)         fixes+="$line"$'\n' ;;
        Performance)   performance+="$line"$'\n' ;;
        Documentation) documentation+="$line"$'\n' ;;
        *)             other+="$line"$'\n' ;;
    esac
# `tformat` and not `format`: the latter *separates* rather than terminates,
# so the last subject arrives without a trailing newline and `read` reports
# end-of-file on it — silently dropping the newest commit of every release.
done < <(git log --no-merges --reverse --pretty=tformat:%s "$from..$to")

emit() {
    [ -n "$2" ] || return 0
    printf '## %s\n\n%s\n' "$1" "$2"
}

emit "Breaking changes" "$breaking"
emit "Features" "$features"
emit "Fixes" "$fixes"
emit "Performance" "$performance"
emit "Documentation" "$documentation"
emit "Other changes" "$other"

if [ -z "$breaking$features$fixes$performance$documentation$other" ]; then
    printf 'No user-facing changes.\n\n'
fi

# The same link `gh --generate-notes` puts at the end, produced here so the
# notes are one thing this script decides rather than two halves stitched
# together by whatever gh does when both are asked for.
remote="$(git remote get-url origin 2>/dev/null || echo '')"
case "$remote" in
    git@github.com:*) remote="https://github.com/${remote#git@github.com:}" ;;
esac
remote="${remote%.git}"
if [ -n "$remote" ]; then
    printf '**Full Changelog**: %s/compare/%s...%s\n' "$remote" "$from" "$to"
fi
