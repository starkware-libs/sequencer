// Security checks for transaction execution (virtual OS version).

// Checks that the number of transactions is one.
func check_n_txs(n_txs: felt) {
    with_attr error_message("Expected exactly one transaction") {
        assert n_txs = 1;
    }
    return ();
}

// Checks that the transaction type is INVOKE_FUNCTION.
func check_tx_type(tx_type: felt) {
    with_attr error_message("Expected INVOKE_FUNCTION transaction") {
        assert tx_type = 'INVOKE_FUNCTION';
    }
    return ();
}
