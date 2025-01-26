# blockifier_reexecution

## Description

The blockier reexecution crate is intended to verify blockifier changes do not break backwards compatibility when executing old blocks. Reexecution of old blocks with the blockifier should output the same expected state-diff as when originally run.

## CLI Commands
Using the different CLI commands, it is possible to run reexecution tests in different modes, to download (permisionless) files for offline reexecution from the GC bucket, and to upload (permissioned) files for offline reexecution to the GC bucket.

### Reexecution Modes

Reexecution can be run via CLI in the following modes: 

- **RPC test:**
Reexecution test where the data required for reexecuting the block, as well as the expected resulting state diff, are retrieved using RPC calls. It is necessary to supply a RPC provider url for these RPC calls. Note that free urls (as, e.g., used by default in the tests) will most likely hit rate limit errors due to the large amount of RPC calls. 
```
cargo run --bin blockifier_reexecution rpc-test -n <node_url> -b <block_number>
```

- **RPC test with preparation for offline reexecution:**
Same as the RPC test; can be executed on multiple blocks. If the block reexecution succeeds, the data required for offline reexecution is saved to a JSON file.
```
cargo run --bin blockifier_reexecution write-to-file -n <node_url> -d <directory_path> -b <block_number_1> ... <block_number_n>
```

- **Offline Reexecution:**
Reexecution test where the data required for reexecuting the block, as well as the expected resulting state diff, are read from local JSON files. Offline reexecution should be run in release mode, as otherwise these tests can be very long. To run offline reexecution on blocks in the GC bucket, first download the files as explained below. Then run
```
cargo run --release --bin blockifier_reexecution reexecute -d <directory_path> -b <optional_block_number_1> ... <optional_block_number_n>
```

### Downloading Offline Reexecution Files from the GC Bucket
Downloading files from the GC bucket requires authentication, by typing in the terminal
`gcloud auth application-default login`

Then, to download the offline reexecution files required for the tests from the gc bucket, in the same shell session run
```
cargo run --bin blockifier_reexecution dowload-files
```
Alternatively, to download only files of specific blocks, run
```
cargo run --bin blockifier_reexecution download-files -b <block_number_1> ... <block_number_n>
```

### Uploading Offline Reexecution Files from the GC Bucket
Uploading files to the GC bucket requires authentication, by typing in the terminal
`gcloud auth application-default login`

Then, to upload the files, in the same shell session run
```
cargo run --bin blockifier_reexecution upload-files -b <block_number_1> ... <block_number_n>
```

## Tests and Test Flags

The 3 main test types in the blockifier_reexecution crate are:

- **Regular (offline\json) tests:** These tests are run as usual by the command
```
cargo test -p blockifier_reexecution
```

- **Full block reexecution tests:** These tests take a long time unless run in release mode. To run these tests, it is first necessary to download the offline reexecution files, as explained above. Hence, these tests are under #[ignore]; they can be run by the command
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

### In order to run all the above tests in a single command:
```
TEST_URL=<node_url> cargo test --release -p blockifier_reexecution --features blockifier_regression_https_testing -- --include-ignored
```

## Adding/Removing Blocks from the Reexecution Test in the CI

To add blocks to the reexecution tests, do the following steps:

- 1. Reexecute the blocks via RPC and write the results to files
```
cargo run --bin blockifier_reexecution write-to-file -n <node_url> -b <block_number_1> ... <block_number_n>
```

- 2. Upload the files to the GC bucket
```
cargo run --bin blockifier_reexecution upload-files -b <block_number_1> ... <block_number_n>
```

- 3. Add the block numbers to the file `block_numbers_for_reexecution.json`

- 4. Add the block numbers to the cases in the test `test_block_reexecution`

To remove blocks from the reexecution tests, simply remove the corresponding block numbers from the file `block_numbers_for_reexecution.json` and remove the corresponding test cases from the test `test_block_reexecution`.

## Changing Reexecution Files' Format
If the files format changes, all the offline reexecution files need to be re-uploaded to the GC bucket. Since files cannot be overwritten in the GC bucket (for backwards compatibility), this requires uploading to a new folder, which is determined by the prefix hash in the file `offline_reexecution_files_prefix`. 
Therefore, when changing the files format, do these 4 steps in order:

- 1. Change the prefix hash in `offline_reexecution_files_prefix`; it is customary, though not mandatory, to use the current commit hash (however, it *must* be a unique string that has not been previously used).

- 2. Run RPC with write to files of all the blocks required for the tests by running the following command
```
cargo run --bin blockifier_reexecution write-to-file -n <node_url>
```
Make sure reexecution of all the blocks succeeded; if necessary, rerun the command with the block numbers that failed.

- 3. Verify that offline reexecution succeeds by running
```
cargo test --release -p blockifier_reexecution -- --ignored
```

- 4. Upload the files by running
```
cargo run --bin blockifier_reexecution upload-files
```
**IMPORTANT:** Do not change the hash in `offline_reexecution_files_prefix` after uploading the files; it is required that the hash point to the files folder location in the gc bucket in order to pass the CI.

