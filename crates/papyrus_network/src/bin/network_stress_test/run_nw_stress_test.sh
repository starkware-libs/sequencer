#!/bin/bash

if [[ -z "$BASE_INSTANCE_NAME" || -z "$PROJECT_ID" || -z "$BASE_PATH" || -z "$ZONE" ]]; then
    echo "Error: BASE_INSTANCE_NAME, PROJECT_ID, ZONE, and PATH must be set."
    echo "Instance name must be set in the format 'base-instance-name-0'... 'base-instance-name-4'."
    exit 1
fi

# Project ID and base instance name
PATH_TO_REPO="$BASE_PATH/sequencer"
PATH_TO_ENV="$BASE_PATH/.cargo/env"

# Commands to execute on each instance
COMMAND_PATH_BOOT="cd ${PATH_TO_REPO} && source ${PATH_TO_ENV} && cargo run --release -p papyrus_network --bin network_stress_test"
COMMAND_PATH="cd ${PATH_TO_REPO} && source ${PATH_TO_ENV} && cargo run --release -p papyrus_network --bin network_stress_test -- crates/papyrus_network/src/bin/network_stress_test/test_config.json"


# Store the process IDs of all opened processes
declare -a process_pids

# Start the boot command on the first instance (instance 0)
(
    echo "Starting boot command on ${BASE_INSTANCE_NAME}-0..."
    gcloud compute ssh "${BASE_INSTANCE_NAME}-0" --project "${PROJECT_ID}" --zone "${ZONE}" -- "${COMMAND_PATH_BOOT}"
    echo "Boot command finished on ${BASE_INSTANCE_NAME}-0."
) &
process_pids+=($!)  # Store the PID of the background process

# Run the command on instances 1 to 4
for i in {1..4}; do
    (
        echo "Starting command on ${BASE_INSTANCE_NAME}-${i}..."
        gcloud compute ssh "${BASE_INSTANCE_NAME}-${i}" --project "${PROJECT_ID}" --zone "${ZONE}" -- "${COMMAND_PATH}"
        echo "Command finished on ${BASE_INSTANCE_NAME}-${i}."
    ) &
    process_pids+=($!)  # Store the PID of the background process
done

# Wait for all commands to complete
for pid in "${process_pids[@]}"; do
    wait "$pid"
done
echo "All commands completed."

# Retrieve the bootstrap output from instance 0
echo "Retrieving bootstrap output from ${BASE_INSTANCE_NAME}-0..."
gcloud compute ssh "${BASE_INSTANCE_NAME}-0" --project "${PROJECT_ID}" --zone "${ZONE}" -- "cat ${PATH_TO_REPO}/crates/papyrus_network/src/bin/network_stress_test/bootstrap_output.csv" > output.csv
echo "Bootstrap output saved as bootstrap_output.csv."

# Retrieve output.csv files from each instance with an incremented filename
for i in {1..4}; do
    (
        echo "Retrieving output.csv from ${BASE_INSTANCE_NAME}-${i}..."
        gcloud compute ssh "${BASE_INSTANCE_NAME}-${i}" --project "${PROJECT_ID}" --zone "${ZONE}" -- "cat ${PATH_TO_REPO}/crates/papyrus_network/src/bin/network_stress_test/output.csv" > output${i}.csv
        echo "Retrieved output.csv from ${BASE_INSTANCE_NAME}-${i} and saved as output${i}.csv."
    ) &
done

# Wait for file retrieval processes to complete
wait
echo "All output files retrieved."
# #!/bin/bash

# # Project ID and base instance name
# PROJECT_ID="starkware-dev"
# BASE_INSTANCE_NAME="eitan-m-broadcast-stress-test"

# # Commands to execute on each instance
# COMMAND_PATH_BOOT="cd /home/eitan_m_starkware_co/sequencer && source /home/eitan_m_starkware_co/.cargo/env && cargo run --release -p papyrus_network --bin network_stress_test"
# COMMAND_PATH="cd /home/eitan_m_starkware_co/sequencer && source /home/eitan_m_starkware_co/.cargo/env && cargo run --release -p papyrus_network --bin network_stress_test -- crates/papyrus_network/src/bin/network_stress_test/test_config.json"

# # Zone for the instances
# ZONE="us-central1-c"

# # Store the process IDs of all opened processes
# declare -a process_pids

# # Start the boot command on the first instance (instance 0)
# (
#     echo "Starting boot command on ${BASE_INSTANCE_NAME}-0..."
#     gcloud compute ssh "${BASE_INSTANCE_NAME}-0" --project "${PROJECT_ID}" --zone "${ZONE}" -- "${COMMAND_PATH_BOOT}"
#     echo "Boot command finished on ${BASE_INSTANCE_NAME}-0."
# ) &
# process_pids+=($!)  # Store the PID of the background process

# # Run the command on instances 1 to 4
# for i in {1..4}; do
#     (
#         echo "Starting command on ${BASE_INSTANCE_NAME}-${i}..."
#         gcloud compute ssh "${BASE_INSTANCE_NAME}-${i}" --project "${PROJECT_ID}" --zone "${ZONE}" -- "${COMMAND_PATH}"
#         echo "Command finished on ${BASE_INSTANCE_NAME}-${i}."
#     ) &
#     process_pids+=($!)  # Store the PID of the background process
# done

# # Wait for all commands to complete
# for pid in "${process_pids[@]}"; do
#     wait "$pid"
# done
# echo "All commands completed."

# # Retrieve the bootstrap output from instance 0
# echo "Retrieving bootstrap output from ${BASE_INSTANCE_NAME}-0..."
# gcloud compute ssh "${BASE_INSTANCE_NAME}-0" --project "${PROJECT_ID}" --zone "${ZONE}" -- 'cat /home/eitan_m_starkware_co/sequencer/crates/papyrus_network/src/bin/network_stress_test/bootstrap_output.csv' > bootstrap_output.csv
# echo "Bootstrap output saved as bootstrap_output.csv."

# # Retrieve output.csv files from each instance with an incremented filename
# for i in {1..4}; do
#     (
#         echo "Retrieving output.csv from ${BASE_INSTANCE_NAME}-${i}..."
#         gcloud compute ssh "${BASE_INSTANCE_NAME}-${i}" --project "${PROJECT_ID}" --zone "${ZONE}" -- 'cat /home/eitan_m_starkware_co/sequencer/crates/papyrus_network/src/bin/network_stress_test/output.csv' > output${i}.csv
#         echo "Retrieved output.csv from ${BASE_INSTANCE_NAME}-${i} and saved as output${i}.csv."
#     ) &
# done

# # Wait for file retrieval processes to complete
# wait
# echo "All output files retrieved."