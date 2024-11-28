## Network Stress Test

# Setup and Run Stress Test

1. **Create Remote Engines**

    Create 5 gcloud VM instances. Make sure to have the necessary RAM and disk space. Each instance should be named in the following pattern:

    ```
    <instance-name>-0, ... ,<instance-name>-4
    ```

2. **Set Bootstrap Node**

    Find the internal IP of your bootstrap node in the VM instances chart on google cloud console. Paste it into the test_config.json file into the bootstrap_peer_multaddr value instead of it's placeholder.

3. **Install Rust and clone repository**

    For all 5 instances run:

    ```
    gcloud compute ssh <instance-name>-0 --project <project-name> -- 'cd <path-to-repo> && sudo apt install -y git unzip clang && curl https://sh.rustup.rs -sSf | sh -s -- -y && source "$HOME/.cargo/env" && git clone https://github.com/starkware-libs/sequencer.git; cd sequencer && sudo scripts/dependencies.sh cargo build --release -p papyrus_network --bin network_stress_test'
    ```
4. **Run test**

    ```
    PROJECT_ID=<project-name> BASE_INSTANCE_NAME=<instance-name> BASE_PATH=<base-path-to-sequencer> ZONE=<zone> ./run_nw_stress_test.sh
    ```

# Pull repo updates to virtual machines

    Run:

    ```
    PROJECT_ID=<project-name> BASE_INSTANCE_NAME=<instance-name> BASE_PATH=<base-path-to-sequencer> ZONE=<zone> ./pull_stress_test.sh
    ```
