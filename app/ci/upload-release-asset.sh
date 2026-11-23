#!/bin/bash
# Uploads files to a GitHub release (a draft too) by its id, replacing assets of the same name (a
# re-run). Needs GH_TOKEN and GITHUB_REPOSITORY.
#
#   app/ci/upload-release-asset.sh <release id> <file>...
set -euo pipefail
id=$1
shift
for file in "$@"; do
  name=$(basename "$file")
  old=$(gh api "repos/$GITHUB_REPOSITORY/releases/$id/assets" --paginate --jq ".[] | select(.name == \"$name\") | .id")
  for asset in $old; do
    gh api -X DELETE "repos/$GITHUB_REPOSITORY/releases/assets/$asset" --silent
  done
  for attempt in 1 2 3; do
    if curl -sS --fail-with-body -X POST \
        -H "Authorization: Bearer $GH_TOKEN" -H "Content-Type: application/octet-stream" \
        --data-binary @"$file" \
        "https://uploads.github.com/repos/$GITHUB_REPOSITORY/releases/$id/assets?name=$name" > /dev/null; then
      echo "uploaded $name ($(wc -c < "$file") bytes)"
      continue 2
    fi
    echo "upload of $name failed (attempt $attempt)"
    sleep 10
  done
  exit 1
done
