#!/bin/bash

# TODO(eitan): support generic amount of instances
if [[ -z "$BASE_INSTANCE_NAME" || -z "$PROJECT_ID" || -z "$ZONE" ]]; then
    echo "Error: BASE_INSTANCE_NAME, PROJECT_ID, and ZONE must be set."
    echo "Each instance name should be of the form <base-instance-name>-<instance-number>, where instance number is between 0 and 4. ie 'papyrus-network-0'"
    exit 1
fi

PATH_TO_REPO="~/sequencer"

# Loop over instances from 0 to 4
for i in {0..4}; do
(
    INSTANCE_NAME="${BASE_INSTANCE_NAME}-${i}"
    echo "Connecting to $INSTANCE_NAME..."
    gcloud compute ssh "$INSTANCE_NAME" --project "$PROJECT_ID" --zone "$ZONE" --tunnel-through-iap -- "cd $PATH_TO_REPO && git pull"
    echo "Finished with $INSTANCE_NAME."
) &
done
echo "All VMs have pulled."
