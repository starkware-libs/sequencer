#!/bin/bash

# TODO(eitan): support generic amount of instances
if [[ -z "$BASE_INSTANCE_NAME" || -z "$PROJECT_ID" || -z "$ZONE" ]]; then
    echo "Error: BASE_INSTANCE_NAME, PROJECT_ID, and ZONE must be set."
    echo "Instance name must be set in the format 'BASE_INSTANCE_NAME-0' -> 'BASE_INSTANCE_NAME-4'."
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
