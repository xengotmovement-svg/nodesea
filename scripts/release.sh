#!/bin/sh
# Release script — bump version, commit, tag, and push to trigger CI.
#
# Usage:
#   ./scripts/release.sh 0.2.0
#   ./scripts/release.sh patch    # 0.1.0 → 0.1.1
#   ./scripts/release.sh minor    # 0.1.0 → 0.2.0
#   ./scripts/release.sh major    # 0.1.0 → 1.0.0

set -e

if [ -z "$1" ]; then
  echo "Usage: $0 <version|patch|minor|major>"
  echo ""
  echo "Examples:"
  echo "  $0 0.2.0     # set exact version"
  echo "  $0 patch     # bump patch (0.1.0 → 0.1.1)"
  echo "  $0 minor     # bump minor (0.1.0 → 0.2.0)"
  echo "  $0 major     # bump major (0.1.0 → 1.0.0)"
  exit 1
fi

# Get current version from Cargo.toml.
CURRENT=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
echo "Current version: $CURRENT"

# Calculate new version.
case "$1" in
  patch)
    MAJOR=$(echo "$CURRENT" | cut -d. -f1)
    MINOR=$(echo "$CURRENT" | cut -d. -f2)
    PATCH=$(echo "$CURRENT" | cut -d. -f3)
    NEW="$MAJOR.$MINOR.$((PATCH + 1))"
    ;;
  minor)
    MAJOR=$(echo "$CURRENT" | cut -d. -f1)
    MINOR=$(echo "$CURRENT" | cut -d. -f2)
    NEW="$MAJOR.$((MINOR + 1)).0"
    ;;
  major)
    MAJOR=$(echo "$CURRENT" | cut -d. -f1)
    NEW="$((MAJOR + 1)).0.0"
    ;;
  *)
    NEW="$1"
    ;;
esac

echo "New version:     $NEW"
echo ""

# Sanity checks.
if [ "$CURRENT" = "$NEW" ]; then
  echo "Version unchanged, aborting."
  exit 1
fi

if ! git diff --quiet; then
  echo "Working tree has uncommitted changes, aborting."
  exit 1
fi

TAG="v$NEW"
if git rev-parse "$TAG" >/dev/null 2>&1; then
  echo "Tag $TAG already exists, aborting."
  exit 1
fi

# Confirm.
printf "Release $TAG? [y/N] "
read -r REPLY
if [ "$REPLY" != "y" ] && [ "$REPLY" != "Y" ]; then
  echo "Aborted."
  exit 1
fi

# Update version in Cargo.toml.
sed -i.bak "s/^version = \"$CURRENT\"/version = \"$NEW\"/" Cargo.toml
rm -f Cargo.toml.bak

# Update Cargo.lock.
cargo check --quiet 2>/dev/null || true

# Commit and tag.
git add Cargo.toml Cargo.lock
git commit -m "release: $TAG"
git tag -a "$TAG" -m "Release $TAG"

echo ""
echo "Created commit and tag $TAG."
echo ""
echo "To publish, run:"
echo "  git push origin main --tags"
