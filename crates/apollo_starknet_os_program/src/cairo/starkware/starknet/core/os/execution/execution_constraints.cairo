// Execution constraints for transaction execution.
// These are no-op implementations for the Sequencer OS.

from starkware.starknet.core.os.block_context import BlockContext

func check_n_txs(n_txs: felt) {
    return ();
}

func check_tx_type(tx_type: felt) {
    return ();
}

func check_sender_address(sender_address: felt, block_context: BlockContext*) {
    return ();
}
