# blockifier_reexecution

## Description

The blockier reexecution crate is intended to verify blockifier changes do not break backwards compatibility when executing old blocks. Reexecution of old blocks with the blockifier should return the same expected state-diff as when originally run.

## Running modes via CLI

Reexecution tests should be run in release mode, as otherwise these tests can be very long. Reexecution can be run via CLI in the following modes: 

- **RPC test:**
Reexecution test where the data required for reexecuting the block, as well as the expected resulting state diff, are received using RPC calls. It is necessary to supply a node url for these RPC calls. Note that free urls (as, e.g., used in the tests) will most likely Err due to the large amount of RPC calls. 
```
cargo run --release --bin blockifier_reexecution rpc-test -n <node_url> -b <block_number>
```

- **RPC test with Writing to JSON file:**
Same as the RPC test; can be executed on multiple blocks. If the block reexecution succeeds, the data required for offline reexecution is saved to a JSON file.
```
cargo run --release --bin blockifier_reexecution write-to-file -n <node_url> -d <directory_path> -b <optional_block_number_1> ... <optional_block_number_n>
```


- **Offline Reexecution:**
Reexecution test where the data required for reexecuting the block, as well as the expected resulting state diff, are read from local JSON files. 
```
cargo run --release --bin blockifier_reexecution reexecute -n <node_url> -d <directory_path> -b <optional_block_number_1> ... <optional_block_number_n>
```

Additionally, uploading\downloading the offline reexecution files can be done via CLI.

- **Upload Files:** 
// TODO

- **Download Files:** 
// TODO

## Tests and test flags

The 3 main test types in the blockifier_reexecution crate are:

- **Regular (offline\json) tests:** These tests are run as usual by the command
```
cargo test -p blockifier_reexecution
```

- **Full block reexecution tests:** These tests take a long time unless run in release mode, hence they are under #[ignore]. These tests can be run by the command
```
cargo test --release -p blockifier_reexecution -- --ignored
```

- **RPC tests:** These tests require sending RPC requests to an RPC url; As these tests use a free node url, they sometimes Err due to RPC Errors. These tests are under the `blockifier_regression_https_testing` flag and are compiled, but not run, in the CI.
To run these tests locally:
```
cargo test -p blockifier_reexecution --features blockifier_regression_https_testing
```

Alternatively, to only compile (without running):
```
cargo test -p blockifier_reexecution --features blockifier_regression_https_testing --no-run
```


In order to run all the above tests in a single command:
```
cargo test --release -p blockifier_reexecution --features blockifier_regression_https_testing -- --include-ignored
```
