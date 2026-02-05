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
popd

# Cleanup
echo "Build complete!"
echo "Cleaning up build context..."
rm -rf "$BUILD_CONTEXT"
