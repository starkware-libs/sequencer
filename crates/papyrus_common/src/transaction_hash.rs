#[cfg(test)]
#[path = "transaction_hash_test.rs"]
mod transaction_hash_test;

use lazy_static::lazy_static;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ascii_as_felt, ChainId};
use starknet_api::transaction::{Transaction, TransactionHash};
use starknet_api::transaction_hash::{get_deprecated_transaction_hashes, get_transaction_hash};
use starknet_api::{StarknetApiError, TransactionOptions};
use starknet_types_core::felt::Felt;

lazy_static! {
    static ref DECLARE: Felt =
        #[allow(clippy::unwrap_used)] ascii_as_felt("declare").unwrap();
    static ref DEPLOY: Felt =
        #[allow(clippy::unwrap_used)] ascii_as_felt("deploy").unwrap();
    static ref DEPLOY_ACCOUNT: Felt =
        #[allow(clippy::unwrap_used)] ascii_as_felt("deploy_account").unwrap();
    static ref INVOKE: Felt =
        #[allow(clippy::unwrap_used)] ascii_as_felt("invoke").unwrap();
    static ref L1_HANDLER: Felt =
        #[allow(clippy::unwrap_used)] ascii_as_felt("l1_handler").unwrap();
    // The first 250 bits of the Keccak256 hash on "constructor".
    // The correctness of this constant is enforced by a test.
    static ref CONSTRUCTOR_ENTRY_POINT_SELECTOR: Felt =
        #[allow(clippy::unwrap_used)]
        Felt::from_hex_unchecked("0x28ffe4ff0f226a9107253e17a904099aa4f63a02a5621de0576e5aa71bc5194");

    pub(crate) static ref ZERO: Felt = Felt::from(0_u8);
    static ref ONE: Felt = Felt::from(1_u8);
    static ref TWO: Felt = Felt::from(2_u8);
    static ref THREE: Felt = Felt::from(3_u8);
}

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
