#!/bin/bash

# A script to automate the release process.
#
# USAGE:
#   ./release.sh {{patch|minor|major}}
#
# This script will:
# 1. Check for a clean git working directory
# 2. Take 'patch', 'minor', or 'major' as an argument to determine the version bump
# 3. Read the current version from Cargo.toml
# 4. Increment the version number
# 5. Update the version in Cargo.toml and documentation links
# 6. Commit the change to Cargo.toml, Cargo.lock, and documentation
# 7. Create a git tag for the new version
# 8. Push the commit and the tag to the remote repository
#
# After the tag is pushed, a GitHub workflow will create the release and pubish it to crates.io

set -e

# Check if the version type argument is provided
if [ -z "$1" ]; then
    echo "Error: Release type not provided" >&2
    echo "Usage: $0 {{patch|minor|major}}" >&2
    exit 1
fi

# Check if the argument is one of the allowed values
if [[ "$1" != "patch" && "$1" != "minor" && "$1" != "major" ]]; then
    echo "Error: Invalid release type '$1'" >&2
    echo "Please use one of: patch, minor, major" >&2
    exit 1
fi

# Check for uncommitted changes in the git repository
if ! git diff-index --quiet HEAD --; then
    echo "Error: Uncommitted changes detected" >&2
    echo "Please commit or stash your changes before creating a release" >&2
    exit 1
fi

echo "✅ No pending changes"

# Get the current version from Cargo.toml using awk to ensure we only get it from the [package] section
current_version=$(awk -F'"' '/^\[package\]/{p=1} p && /^version/{print $2; exit}' Cargo.toml)
if [ -z "$current_version" ]; then
    echo "Error: Could not find version in Cargo.toml's [package] section" >&2
    exit 1
fi
echo "Current version: $current_version"

# Split the version number into its components
IFS='.' read -r -a version_parts <<< "$current_version"
major=${version_parts[0]}
minor=${version_parts[1]}
patch=${version_parts[2]}

# Increment the correct part of the version
case "$1" in
    "major")
        major=$((major + 1))
        minor=0
        patch=0
        ;;
    "minor")
        minor=$((minor + 1))
        patch=0
        ;;
    "patch")
        patch=$((patch + 1))
        ;;
esac

new_version="$major.$minor.$patch"
echo "New version: $new_version"

# Update the version in Cargo.toml using awk
awk -v new_ver="$new_version" '
    BEGIN { FS=OFS="\"" }
    /^\[package\]/ { in_package=1 }
    in_package && /^version/ { $2=new_ver; in_package=0 }
    { print }
' Cargo.toml > Cargo.toml.tmp && mv Cargo.toml.tmp Cargo.toml
cargo update -p intelli-shell
echo "✅ Updated Cargo.toml to version $new_version"

# Update version in documentation links
docs_files=(
    "docs/src/configuration/ai.md"
    "docs/src/configuration/index.md"
)
for doc_file in "${docs_files[@]}"; do
    if [ ! -f "$doc_file" ]; then
        echo "Error: Documentation file not found: $doc_file" >&2
        exit 1
    fi

    if ! grep -q "/blob/v$current_version/default_config.toml" "$doc_file"; then
        echo "Error: Could not find '/blob/v$current_version/default_config.toml' in $doc_file" >&2
        exit 1
    fi

    sed "s|/blob/v$current_version/default_config.toml|/blob/v$new_version/default_config.toml|g" "$doc_file" > "$doc_file.tmp" && mv "$doc_file.tmp" "$doc_file"

    if ! grep -q "/blob/v$new_version/default_config.toml" "$doc_file"; then
        echo "Error: Failed to update version link in $doc_file" >&2
        exit 1
    fi
done
echo "✅ Updated documentation links to version $new_version"

# Verify all anchors and docs links match before committing
cargo test test_docs_configuration_anchors
echo "✅ Verified documentation links and anchors"

# Add the modified Cargo.toml, the updated Cargo.lock, and documentation to git
git add Cargo.toml Cargo.lock "${docs_files[@]}"
echo "✅ Staged Cargo.toml, Cargo.lock, and documentation files"

# Commit the changes
commit_message="chore(release): v$new_version"
git commit -m "$commit_message"
echo "✅ Committed with message: \"$commit_message\""

# Create a new git tag
tag_name="v$new_version"
git tag -a "$tag_name" -m "Release $tag_name"
echo "✅ Created git tag: $tag_name"

# Push the commit and the tag to the remote repository
echo "Pushing changes and tags to remote..."
git push
git push origin "$tag_name"

echo "🚀 Release $new_version successfully pushed to remote"
