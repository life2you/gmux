#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DEFAULT_FORMULA_PATH="$REPO_ROOT/packaging/homebrew-tap/Formula/gmux.rb"

OWNER="${OWNER:-life2you}"
REPO="${REPO:-gmux}"
VERSION=""
FORMULA_PATH="$DEFAULT_FORMULA_PATH"
DRY_RUN=0
TMP_FORMULA_PATH=""

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

show_help() {
  cat <<EOF
Usage: ./scripts/update-homebrew-formula.sh [options] [version]

Options:
  --output PATH   Write the generated formula to PATH.
  --dry-run       Print the generated formula to stdout instead of writing it.
  --help          Show this help message.

Defaults:
  output path: $DEFAULT_FORMULA_PATH
  version:     inferred from Cargo.toml when omitted
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --output)
      if [[ $# -lt 2 ]]; then
        echo "Missing value for --output" >&2
        exit 1
      fi
      FORMULA_PATH="$2"
      shift 2
      ;;
    --output=*)
      FORMULA_PATH="${1#*=}"
      shift
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --help)
      show_help
      exit 0
      ;;
    --)
      shift
      if [[ $# -gt 0 ]]; then
        VERSION="$1"
        shift
      fi
      break
      ;;
    -*)
      echo "Unknown option: $1" >&2
      echo "Use --help to see supported options." >&2
      exit 1
      ;;
    *)
      if [[ -n "$VERSION" ]]; then
        echo "Unexpected extra argument: $1" >&2
        exit 1
      fi
      VERSION="$1"
      shift
      ;;
  esac
done

if [[ $# -gt 0 ]]; then
  echo "Unexpected extra arguments: $*" >&2
  exit 1
fi

VERSION="${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' "$REPO_ROOT/Cargo.toml" | head -n1)}"

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

FORMULA_CONTENT="$(cat <<EOF
class Gmux < Formula
  desc "Terminal Git workflow tool for multi-env branch sync and GitLab MR automation"
  homepage "https://github.com/$OWNER/$REPO"
  url "$URL"
  sha256 "$SHA256"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args(path: ".")
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/gmux --version")
  end
end
EOF
)"

if [[ "$DRY_RUN" == "1" ]]; then
  printf '%s\n' "$FORMULA_CONTENT"
  {
    echo "Dry run only."
    echo "Version: $VERSION"
    echo "SHA256:  $SHA256"
    echo "Output:  $FORMULA_PATH"
  } >&2
  exit 0
fi

mkdir -p "$(dirname "$FORMULA_PATH")"
TMP_FORMULA_PATH="$(mktemp "$(dirname "$FORMULA_PATH")/$(basename "$FORMULA_PATH").XXXXXX")"

cleanup() {
  rm -f "$TMP_FORMULA_PATH"
}
trap cleanup EXIT

printf '%s\n' "$FORMULA_CONTENT" > "$TMP_FORMULA_PATH"
mv "$TMP_FORMULA_PATH" "$FORMULA_PATH"

echo "Updated $FORMULA_PATH"
echo "Version: $VERSION"
echo "SHA256:  $SHA256"
