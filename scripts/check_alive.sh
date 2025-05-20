#!/bin/bash

# Default values
ADDRESS="http://localhost:8082/monitoring/alive"
TIMEOUT=60
INTERVAL=5
INITIAL_DELAY=10
RETRY=3
RETRY_DELAY=1

# Help function
function show_help() {
  echo "Usage: $0 [OPTIONS]"
  echo ""
  echo "Options:"
  echo "  --address ADDRESS        Address to check (default: $ADDRESS)"
  echo "  --timeout TIMEOUT        Timeout duration in seconds (default: $TIMEOUT)"
  echo "  --interval INTERVAL      Interval between checks in seconds (default: $INTERVAL)"
  echo "  --initial-delay DELAY    Initial delay before starting the checks (default: $INITIAL_DELAY)"
  echo "  --retry RETRY            Number of retries in case of failure (default: $RETRY)"
  echo "  --retry-delay DELAY      Delay between retries in seconds (default: $RETRY_DELAY)"
  echo "  --help                   Show this help message"
  echo ""
}

# Parse arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --address)
      ADDRESS="$2"
      shift 2
      ;;
    --timeout)
      TIMEOUT="$2"
      shift 2
      ;;
    --interval)
      INTERVAL="$2"
      shift 2
      ;;
    --initial-delay)
      INITIAL_DELAY="$2"
      shift 2
      ;;
    --retry)
      RETRY="$2"
      shift 2
      ;;
    --retry-delay)
      RETRY_DELAY="$2"
      shift 2
      ;;
    --help)
      show_help
      exit 0
      ;;
    *)
      echo "Unknown option: $1"
      show_help
      exit 1
      ;;
  esac
done

sleep "$INITIAL_DELAY"

# Start time
START_TIME=$(date +%s)

echo "Starting live check test"
echo "start_time: $(date -d @"$START_TIME" '+%Y-%m-%d %H:%M:%S')"
echo "address: $ADDRESS:$PORT"
echo "timeout_sec: $TIMEOUT"
echo "interval_sec: $INTERVAL"
echo "initial_delay_sec: $INITIAL_DELAY"
echo "retry: $RETRY"
echo "retry_delay: $RETRY_DELAY"
echo ""

# Main loop
while true; do
  # Check elapsed time
  ELAPSED_TIME=$(( $(date +%s) - START_TIME ))
  if [ $ELAPSED_TIME -ge "$TIMEOUT" ]; then
    echo "Successfully ran for $TIMEOUT seconds!"
    break
  fi

  # Run curl command with output suppressed
  echo "Calling ${ADDRESS}..."
  response=$(curl --retry "${RETRY}" --retry-delay "${RETRY_DELAY}" -s -X GET "${ADDRESS}")

  exit_code="$?"
  if [ $exit_code -ne 0 ]; then
    echo "Failed to call ${ADDRESS}"
    exit $exit_code
  else
    echo "$response"
  fi

  # Sleep for the specified interval
  echo -e "Sleeping $INTERVAL seconds before next call.\n"
  sleep "$INTERVAL"
done

exit 0
