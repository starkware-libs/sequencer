#!/usr/bin/env bash

IMAGE_NAME="$1"

if [ -z "$IMAGE_NAME" ]; then
  echo "❌ No image name provided."
  exit 1
fi

echo "🚀 Running container from image: $IMAGE_NAME"

# Run a test or command inside the container
docker run --rm "$IMAGE_NAME" echo "✅ Container started successfully with $IMAGE_NAME"
