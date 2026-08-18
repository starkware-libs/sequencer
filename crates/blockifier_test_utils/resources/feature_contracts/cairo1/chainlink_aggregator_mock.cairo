// Mock of Chainlink's Starknet price feed aggregator, for tests that read a feed through the
// batcher's view call path.
//
// `latest_round_data` and `decimals` carry the real aggregator's entry point names and return
// layouts; the oracle's decoder is written against those, so a test that reads this contract
// checks the decoder against a deployed contract rather than against a hand-written fixture.
//
// Both entry points report what the contract's storage holds, and a test seeds that storage per
// deployed instance to drive the oracle's guards: staleness in either direction, a mis-scaled
// answer, and a zero answer. The layout a test writes is the `starknet::Store` layout of the
// members below: `round`'s five fields occupy five consecutive slots from the base slot of `round`,
// in declaration order, and `feed_decimals` occupies the base slot of `feed_decimals`.

// The five fields Chainlink's aggregator returns from `latest_round_data`, in the order it returns
// them. `round_id` is phase-encoded as `(phase_id << 128) | aggregator_round_id`, so it is a
// `felt252` rather than an integer type.
#[derive(Copy, Drop, Serde, starknet::Store)]
pub struct Round {
    pub round_id: felt252,
    pub answer: u128,
    pub block_num: u64,
    pub started_at: u64,
    pub updated_at: u64,
}

// The two entry points the oracle calls, with the real aggregator's signatures.
#[starknet::interface]
pub trait IChainlinkAggregator<TContractState> {
    fn latest_round_data(self: @TContractState) -> Round;
    fn decimals(self: @TContractState) -> u8;
}

#[starknet::contract]
mod ChainlinkAggregatorMock {
    use starknet::storage::StoragePointerReadAccess;
    use super::Round;

    #[storage]
    struct Storage {
        round: Round,
        // Named apart from the `decimals` entry point, which a storage member of that name would
        // collide with.
        feed_decimals: u8,
    }

    #[abi(embed_v0)]
    impl ChainlinkAggregatorImpl of super::IChainlinkAggregator<ContractState> {
        fn latest_round_data(self: @ContractState) -> Round {
            self.round.read()
        }

        fn decimals(self: @ContractState) -> u8 {
            self.feed_decimals.read()
        }
    }
}
