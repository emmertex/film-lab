#!/bin/sh
# Releases Film Lab: bumps the version, commits it, tags it and pushes both, which starts
# .github/workflows/release.yml (the CI checks, then a GitHub release with the plug-ins).
#
#   scripts/release.sh              # minor: 0.1.0 → 0.2.0
#   scripts/release.sh patch        # 0.1.0 → 0.1.1
#   scripts/release.sh major        # 0.1.0 → 1.0.0
#   scripts/release.sh 0.2.0-rc.1   # an exact version (a suffix makes a pre-release)
#   scripts/release.sh 0.1.0        # the current version: no bump, only the tag (a first release)
#
# One version for everything: [workspace.package] in Cargo.toml and in crates/photocraft/Cargo.toml
# (the plug-ins' manifests report the latter). Needs a clean tree on main, not behind origin.
# Runs the CI checks before committing and asks before pushing; --yes skips the question,
# --no-checks the checks.
set -eu
cd "$(dirname "$0")/.."

die() {
    echo "release: $*" >&2
    exit 1
}

bump="minor"
yes=false
checks=true
for arg in "$@"; do
    case "$arg" in
        --yes | -y) yes=true ;;
        --no-checks) checks=false ;;
        -h | --help)
            # The comment block at the top of this file.
            awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "$0"
            exit 0
            ;;
        -*) die "unknown option $arg" ;;
        *) bump=$arg ;;
    esac
done

# The version in [workspace.package]: the first line that starts with `version = `.
version_of() {
    sed -n 's/^version = "\(.*\)"$/\1/p' "$1" | head -n 1
}
set_version() {
    awk -v v="$2" '!done && /^version = "/ { print "version = \"" v "\""; done = 1; next } { print }' "$1" >"$1.tmp"
    mv "$1.tmp" "$1"
}

current=$(version_of crates/photocraft/Cargo.toml)
[ -n "$current" ] || die "no version in crates/photocraft/Cargo.toml"
[ "$(version_of Cargo.toml)" = "$current" ] || die "Cargo.toml says $(version_of Cargo.toml), crates/photocraft/Cargo.toml says $current; make them agree first"

semver='^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$'
base=${current%%-*}
major=${base%%.*}
rest=${base#*.}
minor=${rest%%.*}
patch=${rest#*.}
case "$bump" in
    major) next="$((major + 1)).0.0" ;;
    minor) next="$major.$((minor + 1)).0" ;;
    # A pre-release's patch is its own version: 0.2.0-rc.1 → 0.2.0.
    patch) if [ "$base" != "$current" ]; then next=$base; else next="$major.$minor.$((patch + 1))"; fi ;;
    *)
        echo "$bump" | grep -Eq "$semver" || die "expected patch, minor, major or a version like 1.2.3 (got $bump)"
        next=$bump
        ;;
esac
tag="v$next"

# The state of the repository.
branch=$(git rev-parse --abbrev-ref HEAD)
[ "$branch" = main ] || die "on $branch; releases are made from main"
[ -z "$(git status --porcelain)" ] || die "the working tree has changes; commit or stash them first"
git fetch --quiet --tags origin
[ -z "$(git rev-list HEAD..origin/main 2>/dev/null)" ] || die "main is behind origin/main; pull first"
git rev-parse -q --verify "refs/tags/$tag" >/dev/null && die "tag $tag already exists"
[ -z "$(git ls-remote --tags origin "refs/tags/$tag")" ] || die "tag $tag already exists on origin"

echo "release: $current → $next ($tag)"

if [ "$next" != "$current" ]; then
    set_version Cargo.toml "$next"
    set_version crates/photocraft/Cargo.toml "$next"
    # Rewrite both lockfiles for the new version (nothing else is updated).
    cargo metadata --format-version 1 >/dev/null
    cargo metadata --format-version 1 --manifest-path crates/photocraft/Cargo.toml >/dev/null
fi

if $checks; then
    echo "release: running the CI checks"
    cargo fmt --all --check
    cargo fmt --all --check --manifest-path crates/photocraft/Cargo.toml
    cargo clippy --quiet --workspace --all-targets --locked -- -D warnings
    cargo clippy --quiet --manifest-path crates/photocraft/Cargo.toml --target wasm32-unknown-unknown --locked -- -D warnings
    cargo test --quiet --workspace --locked
    crates/photocraft/build.sh --locked
fi

if [ "$next" != "$current" ]; then
    git add Cargo.toml Cargo.lock crates/photocraft/Cargo.toml crates/photocraft/Cargo.lock
    git commit --quiet -m "Release $next"
fi
git tag -a "$tag" -m "Film Lab $next"

if ! $yes; then
    printf 'release: push main and %s to origin? This publishes the release. [y/N] ' "$tag"
    read -r answer
    case "$answer" in
        y | Y | yes) ;;
        *)
            echo "release: not pushed. To undo: git tag -d $tag$([ "$next" != "$current" ] && echo " && git reset --hard HEAD~1")"
            exit 1
            ;;
    esac
fi
git push --atomic origin main "$tag"
echo "release: pushed $tag"
repo=$(git remote get-url origin | sed -n -E 's#.*github\.com[:/](.*)$#\1#p' | sed 's#\.git$##')
[ -z "$repo" ] || echo "release: follow it at https://github.com/$repo/actions"
