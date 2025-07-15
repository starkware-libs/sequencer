#[derive(Drop, Serde, starknet::Store)]
pub struct Staker {
    pub address: felt252,
    pub staked_amount: u128,
    pub pubkey: felt252,
}

#[derive(Drop, Serde, starknet::Store)]
pub struct Epoch {
    pub epoch: u64,
    pub start_block: u64,
    pub end_block: u64,
}

#[starknet::interface]
pub trait IStaking<TContractState> {
    fn add_staker(ref self: TContractState, staker: Staker);
    fn set_stakers(ref self: TContractState, stakers: Array<Staker>);
    fn get_stakers(self: @TContractState, epoch: u64) -> Array<Staker>;
    fn set_current_epoch(ref self: TContractState, epoch: Epoch);
    fn get_current_epoch(self: @TContractState) -> Epoch;
}

#[starknet::contract]
mod Staking {
    use starknet::storage::{
        MutableVecTrait, StoragePointerReadAccess, StoragePointerWriteAccess, Vec, VecTrait,
    };
    use super::{Epoch, Staker};

    #[storage]
    struct Storage {
        stakers: Vec<Staker>,
        current_epoch: Epoch,
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

        fn set_current_epoch(ref self: ContractState, epoch: Epoch) {
            self.current_epoch.write(epoch);
        }

        fn get_current_epoch(self: @ContractState) -> Epoch {
            self.current_epoch.read()
        }
    }
}
