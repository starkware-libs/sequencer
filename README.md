# Sequencer

Sequencer for Starknet, currently in development.

## Development

Run [the dependencies script](scripts/dependencies.sh) to setup your environment.

## Testing

Build and run tests for a specific package:

```bash
cargo build -p <package>
SEED=0 cargo test -p <package>
```

Run the full pre-merge checks against changed files:

```bash
python scripts/run_tests.py --command clippy --changes_only --commit_id HEAD
python scripts/run_tests.py --command integration --changes_only --include_dependencies --commit_id HEAD
```
