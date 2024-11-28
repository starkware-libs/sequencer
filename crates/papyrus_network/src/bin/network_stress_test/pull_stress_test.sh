#!/bin/bash

if [[ -z "$BASE_INSTANCE_NAME" || -z "$PROJECT_ID" || -z "$BASE_PATH" || -z "$ZONE" ]]; then
    echo "Error: BASE_INSTANCE_NAME, PROJECT_ID, and PATH must be set."
    echo "Instance name must be set in the format 'base-instance-name-0'... 'base-instance-name-4'."
    exit 1
fi

PATH_TO_REPO="$BASE_PATH/sequencer"

# Loop over instances from 0 to 4
for i in {0..4}; do
(   
 INSTANCE_NAME="${BASE_INSTANCE_NAME}-${i}"
    echo "Connecting to $INSTANCE_NAME..."
    
    gcloud compute ssh "$INSTANCE_NAME" --project "$PROJECT_ID" --zone "$ZONE" -- "cd $PATH_TO_REPO && git pull"
    
    echo "Finished with $INSTANCE_NAME."
) &
done
