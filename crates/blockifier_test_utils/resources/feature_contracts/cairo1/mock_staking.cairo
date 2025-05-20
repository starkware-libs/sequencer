#[derive(Drop, Serde, starknet::Store)]
pub struct Staker {
    pub address: felt252,
    pub staked_amount: u128,
    pub pubkey: felt252,
}

#[starknet::interface]
pub trait IStaking<TContractState> {
    fn add_staker(ref self: TContractState, staker: Staker);
    fn set_stakers(ref self: TContractState, stakers: Array<Staker>);
    fn get_stakers(self: @TContractState, epoch: u64) -> Array<Staker>;
}

#[starknet::contract]
mod Staking {
    use starknet::storage::{MutableVecTrait, StoragePointerReadAccess, Vec, VecTrait};
    use super::Staker;

    #[storage]
    struct Storage {
        stakers: Vec<Staker>,
    }

    #[abi(embed_v0)]
    impl StakingImpl of super::IStaking<ContractState> {
        fn add_staker(ref self: ContractState, staker: Staker) {
            self.stakers.push(staker);
        }

        fn set_stakers(ref self: ContractState, stakers: Array<Staker>) {
            for _ in 0..self.stakers.len() {
                let _ = self.stakers.pop();
            }
            assert(self.stakers.len() == 0, 'Stakers vec is not empty');
            for staker in stakers {
                self.add_staker(staker);
            }
        }

        // epoch is not used in this mock, but should be part of the interface.
        fn get_stakers(self: @ContractState, epoch: u64) -> Array<Staker> {
            let mut stakers = array![];
            for i in 0..self.stakers.len() {
                stakers.append(self.stakers.at(i).read());
            }
            stakers
        }
    }
}
