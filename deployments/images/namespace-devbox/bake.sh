#!/bin/bash
set -e

# Configuration
# Find repo root by walking up the directory tree looking for marker files
find_repo_root() {
  local dir="$1"
  while [ "$dir" != "/" ]; do
    # Check for common repo root markers
    if [ -f "$dir/Cargo.toml" ] && [ -f "$dir/WORKSPACE" ]; then
      echo "$dir"
      return 0
    fi
    # Also check for .git as fallback
    if [ -d "$dir/.git" ]; then
      echo "$dir"
      return 0
    fi
    local parent="$(dirname "$dir")"
    # Prevent infinite loop if dirname doesn't change
    if [ "$parent" = "$dir" ]; then
      break
    fi
    dir="$parent"
  done
  return 1
}

# Check for required tools.
check_requirements() {
  local missing=()

  if ! command -v nsc &> /dev/null; then
    missing+=("nsc")
  fi

  if ! command -v devbox &> /dev/null; then
    missing+=("devbox")
  fi

  if ! command -v rsync &> /dev/null; then
    missing+=("rsync")
  fi

  if ! command -v jq &> /dev/null; then
    missing+=("jq")
  fi

  if [ ${#missing[@]} -ne 0 ]; then
    echo "Error: Missing required tools: ${missing[*]}" >&2
    echo "Please install them and try again." >&2
    exit 1
  fi
}

check_requirements

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(find_repo_root "$SCRIPT_DIR")"
if [ -z "$REPO_ROOT" ]; then
  echo "Error: Could not find repository root" >&2
  exit 1
fi

BUILD_CONTEXT="/tmp/build-context"
IMAGE_NAME=""

# Parse command line arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --name)
      IMAGE_NAME="$2"
      shift 2
      ;;
    --help|-h)
      echo "Usage: $0 --name IMAGE_NAME"
      echo ""
      echo "Options:"
      echo "  --name IMAGE_NAME    Name for the devbox image (required)"
      echo "  --help, -h           Show this help message"
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      echo "Use --help for usage information" >&2
      exit 1
      ;;
  esac
done

# Validate that --name was provided
if [ -z "$IMAGE_NAME" ]; then
  echo "Error: --name flag is required" >&2
  echo "Use --help for usage information" >&2
  exit 1
fi

# Clean up any existing build context
if [ -d "$BUILD_CONTEXT" ]; then
  rm -rf "$BUILD_CONTEXT"
fi

echo "Creating build context directory..."
mkdir -p "$BUILD_CONTEXT"

echo "Creating build context without excluded files..."

# Copy files excluding unwanted directories
rsync -av \
  --exclude='.git' \
  --exclude='.gitignore' \
  --exclude='target' \
  --exclude='**/target' \
  --exclude='data' \
  --exclude='logs' \
  --exclude='.venv' \
  --exclude='**/.venv' \
  --exclude='venv' \
  --exclude='**/venv' \
  --exclude='node_modules' \
  --exclude='**/node_modules' \
  --exclude='__pycache__' \
  --exclude='**/__pycache__' \
  --exclude='*.pyc' \
  --exclude='.pytest_cache' \
  --exclude='.mypy_cache' \
  --exclude='.DS_Store' \
  --exclude='*.swp' \
  --exclude='*.swo' \
  "$REPO_ROOT/" "$BUILD_CONTEXT/"

echo "Build context created at $BUILD_CONTEXT"
echo "Combining Dockerfiles..."

# Combine base Dockerfile with namespace-devbox Dockerfile
pushd "$BUILD_CONTEXT"
{
  # Read entire base Dockerfile
  cat "deployments/images/base/Dockerfile"
  echo ""
  # Read entire namespace-devbox Dockerfile
  cat "deployments/images/namespace-devbox/Dockerfile"
} > Dockerfile

echo "Building devbox image..."
devbox image build . --name "$IMAGE_NAME"
exit_code=$?
if [ $exit_code -ne 0 ]; then
  echo "Error: Failed to build devbox image" >&2
  exit 1
fi
popd

# Fetch image reference from the registry.
echo "Fetching image reference from registry..."
IMAGE_REFERENCE=$(nsc registry list --repository "$IMAGE_NAME" -o json \
  | jq -r '.[] | select(.tags != null and (.tags[] == "devbox-latest")) | .image_ref')

if [ -z "$IMAGE_REFERENCE" ] || [ "$IMAGE_REFERENCE" = "null" ]; then
  echo "Error: Could not find image with tag 'devbox-latest' in repository '$IMAGE_NAME'" >&2
  exit 1
fi

# Extend image expiration to 10 years from now.
EXPIRE_AT=$(date -u -d "+10 years" '+%Y-%m-%dT%H:%M:%SZ' 2>/dev/null \
  || date -u -v+10y '+%Y-%m-%dT%H:%M:%SZ')
nsc registry update-image-expiration "$IMAGE_REFERENCE" --expire-at "$EXPIRE_AT"

exit_code=$?
if [ $exit_code -ne 0 ]; then
  echo "Warning: Failed to update image expiration" >&2
  echo "The image will expire in 7 days." >&2
  echo "To extend the expiration, run the following command:" >&2
  echo "nsc registry update-image-expiration <IMAGE_REFERENCE> --expire-at <EXPIRE_AT>" >&2
fi

echo "Build complete!"
echo "Image reference: $IMAGE_REFERENCE"

# Cleanup
echo "Cleaning up build context..."
rm -rf "$BUILD_CONTEXT"
