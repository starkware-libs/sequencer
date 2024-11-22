# blockifier_reexecution

## Description

The blockier reexecution crate is intended to verify blockifier changes do not break backwards compatibility when executing old blocks. Reexecution of old blocks with the blockifier should output the same expected state-diff as when originally run.

## Running modes via CLI

Reexecution can be run via CLI in the following modes: 

- **RPC test:**
Reexecution test where the data required for reexecuting the block, as well as the expected resulting state diff, are retrieved using RPC calls. It is necessary to supply a RPC provider url for these RPC calls. Note that free urls (as, e.g., used in the tests) will most likely hit rate limit errors due to the large amount of RPC calls. 
```
cargo run --bin blockifier_reexecution rpc-test -n <node_url> -b <block_number>
```

- **RPC test with preparation for offline reexecution:**
Same as the RPC test; can be executed on multiple blocks. If the block reexecution succeeds, the data required for offline reexecution is saved to a JSON file.
```
cargo run --bin blockifier_reexecution write-to-file -n <node_url> -d <directory_path> -b <optional_block_number_1> ... <optional_block_number_n>
```


- **Offline Reexecution:**
Reexecution test where the data required for reexecuting the block, as well as the expected resulting state diff, are read from local JSON files. Offline reexecution should be run in release mode, as otherwise these tests can be very long.
```
cargo run --release --bin blockifier_reexecution reexecute -n <node_url> -d <directory_path> -b <optional_block_number_1> ... <optional_block_number_n>
```

// TODO(Aner): seperate between uploading (permissioned) and downloading (permisionless).
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

- **RPC tests:** These tests check that RPC responses can be properly deserialized. They require sending RPC requests to an RPC provider; These tests are under the `blockifier_regression_https_testing` flag and are compiled, but not run, in the CI.
These tests use a free node url by default, so they sometimes hit rate limit errors; hence, it is recommended to use your own RPC provider (set in the `TEST_URL` environment variable).
To run these tests locally:
```
TEST_URL=<node_url> cargo test -p blockifier_reexecution --features blockifier_regression_https_testing
```

Alternatively, to only compile (without running):
```
cargo test -p blockifier_reexecution --features blockifier_regression_https_testing --no-run
```


In order to run all the above tests in a single command:
```
TEST_URL=<node_url> cargo test --release -p blockifier_reexecution --features blockifier_regression_https_testing -- --include-ignored
```

## Adding/removing blocks from the reexecution test in the CI
//TODO
