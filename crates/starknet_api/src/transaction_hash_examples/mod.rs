//! Examples for computing transaction hashes from JSON transaction files.

#[cfg(test)]
mod tests {
    use crate::core::ChainId;
    use crate::test_utils::read_json_file;
    use crate::transaction::{
        InvokeTransaction,
        Transaction,
        TransactionHasher,
        TransactionVersion,
    };

    const CHAIN_ID: ChainId = ChainId::Mainnet;

    #[test]
    fn test_compute_invoke_v3_transaction_hashes() {
        // Load transactions from JSON files
        let tx1: Transaction = read_json_file("transaction_hash_examples/invoke_v3_tx1.json");
        let tx2: Transaction = read_json_file("transaction_hash_examples/invoke_v3_tx2.json");

        // Extract InvokeTransactionV3 from the transactions
        let Transaction::Invoke(InvokeTransaction::V3(invoke_tx1)) = tx1 else {
            panic!("Expected InvokeTransactionV3");
        };
        let Transaction::Invoke(InvokeTransaction::V3(invoke_tx2)) = tx2 else {
            panic!("Expected InvokeTransactionV3");
        };

        // Compute transaction hashes
        let version = TransactionVersion::THREE;

        let hash1 = invoke_tx1.calculate_transaction_hash(&CHAIN_ID, &version).unwrap();
        let hash2 = invoke_tx2.calculate_transaction_hash(&CHAIN_ID, &version).unwrap();

        println!("Transaction 1 hash: {:#x}", hash1.0);
        println!("Transaction 2 hash: {:#x}", hash2.0);
    }
}
