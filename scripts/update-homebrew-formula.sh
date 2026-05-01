#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
FORMULA_PATH="$REPO_ROOT/packaging/homebrew-tap/Formula/gmux.rb"
TMP_FORMULA_PATH="$(mktemp "$REPO_ROOT/packaging/homebrew-tap/Formula/gmux.rb.XXXXXX")"

cleanup() {
  rm -f "$TMP_FORMULA_PATH"
}
trap cleanup EXIT

OWNER="${OWNER:-life2you}"
REPO="${REPO:-gmux}"
VERSION="${1:-$(sed -n 's/^version = "\(.*\)"/\1/p' "$REPO_ROOT/Cargo.toml" | head -n1)}"

require_command() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "Required command not found: $cmd" >&2
    exit 1
  fi
}

require_command git
require_command curl
require_command shasum

if [[ -z "$VERSION" ]]; then
  echo "Failed to detect version from Cargo.toml" >&2
  exit 1
fi

TAG="v$VERSION"
URL="https://github.com/$OWNER/$REPO/archive/refs/tags/$TAG.tar.gz"
LOCAL_TAG_COMMIT="$(git -C "$REPO_ROOT" rev-parse "$TAG^{}" 2>/dev/null || true)"

if [[ -z "$LOCAL_TAG_COMMIT" ]]; then
  echo "Local tag $TAG does not exist. Create and push the release tag first." >&2
  exit 1
fi

TAG_VERSION="$(
  git -C "$REPO_ROOT" show "$TAG:Cargo.toml" |
    sed -n 's/^version = "\(.*\)"/\1/p' |
    head -n1
)"

if [[ "$TAG_VERSION" != "$VERSION" ]]; then
  echo "Tag $TAG contains Cargo.toml version ${TAG_VERSION:-<unknown>}, expected $VERSION." >&2
  echo "Refusing to generate a formula from a tag whose source version does not match the requested release." >&2
  exit 1
fi

REMOTE_TAG_COMMIT="$(git -C "$REPO_ROOT" ls-remote --tags origin "refs/tags/$TAG^{}" | awk 'NR==1 {print $1}')"
if [[ -z "$REMOTE_TAG_COMMIT" ]]; then
  REMOTE_TAG_COMMIT="$(git -C "$REPO_ROOT" ls-remote --tags origin "refs/tags/$TAG" | awk 'NR==1 {print $1}')"
fi

if [[ -z "$REMOTE_TAG_COMMIT" ]]; then
  echo "Remote tag $TAG not found on origin. Push the tag before updating the formula." >&2
  exit 1
fi

if [[ "$REMOTE_TAG_COMMIT" != "$LOCAL_TAG_COMMIT" ]]; then
  echo "Remote tag $TAG points to $REMOTE_TAG_COMMIT, but local tag points to $LOCAL_TAG_COMMIT." >&2
  echo "Push the corrected tag before updating the formula." >&2
  exit 1
fi

SHA256="$(
  curl --fail --silent --show-error --location --retry 3 "$URL" |
    shasum -a 256 |
    awk '{print $1}'
)"

cat > "$TMP_FORMULA_PATH" <<EOF
class Gmux < Formula
  desc "Terminal Git workflow tool for multi-env branch sync and GitLab MR automation"
  homepage "https://github.com/$OWNER/$REPO"
  url "$URL"
  sha256 "$SHA256"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", "--locked", *std_cargo_args(path: ".")
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/gmux --version")
  end
end
EOF

mv "$TMP_FORMULA_PATH" "$FORMULA_PATH"

echo "Updated $FORMULA_PATH"
echo "Version: $VERSION"
echo "SHA256:  $SHA256"
