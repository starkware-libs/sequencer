# Transaction Hash Examples

This module contains examples for computing transaction hashes from JSON-formatted transactions.

## Structure

- `mod.rs` - Contains test function that compute transaction hashes
- `../../resources/transaction_hash_examples/` - Contains JSON transaction files

## JSON Files

- `invoke_v3_tx1.json`
- `invoke_v3_tx2.json`

## Running the Tests

To compute the transaction hashes and see the output:

```bash
cargo test -p starknet_api transaction_hash_examples -- --nocapture
```

## Expected Output

```
Transaction 1 hash: 0x2064a4dec0242812bd3b83a4427789b440dfe67f916e58125ed789994a8bfd3
Transaction 2 hash: 0x2498212fcfe9eac2050201908fd4fcbc5424b86363ab12d8c8f9af488b72347
```
