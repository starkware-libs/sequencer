#[cfg(test)]
#[path = "transaction_hash_test.rs"]
mod transaction_hash_test;

use starknet_api::block::BlockNumber;
use starknet_api::core::ChainId;
use starknet_api::transaction::{Transaction, TransactionHash};
use starknet_api::transaction_hash::{get_deprecated_transaction_hashes, get_transaction_hash};
use starknet_api::{StarknetApiError, TransactionOptions};

/// Validates the hash of a starknet transaction.
/// For transactions on testnet or those with a low block_number, we validate the
/// transaction hash against all potential historical hash computations. For recent
/// transactions on mainnet, the hash is validated by calculating the precise hash
/// based on the transaction version.
pub fn validate_transaction_hash(
    transaction: &Transaction,
    block_number: &BlockNumber,
    chain_id: &ChainId,
    expected_hash: TransactionHash,
    transaction_options: &TransactionOptions,
) -> Result<bool, StarknetApiError> {
    let mut possible_hashes = get_deprecated_transaction_hashes(
        chain_id,
        block_number,
        transaction,
        transaction_options,
    )?;
    possible_hashes.push(get_transaction_hash(transaction, chain_id, transaction_options)?);
    Ok(possible_hashes.contains(&expected_hash))
}
