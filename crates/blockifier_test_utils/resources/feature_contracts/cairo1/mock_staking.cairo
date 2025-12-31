use starknet::ContractAddress;

pub type BlockNumber = u64;
pub type Epoch = u64;
pub type PublicKey = felt252;
pub type StakingPower = u128;

#[derive(Drop, Serde, starknet::Store)]
pub struct Staker {
    pub contract_address: ContractAddress,
    pub staking_power: StakingPower,
    pub pub_key: Option<PublicKey>,
}

#[derive(Drop, Serde, starknet::Store)]
pub struct EpochInfo {
    pub epoch_id: Epoch,
    pub start_block: BlockNumber,
    pub epoch_length: u32,
}

#[starknet::interface]
pub trait IStaking<TContractState> {
    fn add_staker(ref self: TContractState, staker: Staker);
    fn set_stakers(ref self: TContractState, stakers: Array<Staker>);
    fn set_current_epoch(ref self: TContractState, epoch: EpochInfo);

    // The following functions have exactly the same interface as the real Staking contract.
    fn get_stakers(
        self: @TContractState, epoch_id: Epoch,
    ) -> Span<(ContractAddress, StakingPower, Option<PublicKey>)>;
    fn get_current_epoch_data(self: @TContractState) -> (Epoch, BlockNumber, u32);
}

#[starknet::contract]
mod Staking {
    use starknet::ContractAddress;
    use starknet::storage::{
        MutableVecTrait, StoragePointerReadAccess, StoragePointerWriteAccess, Vec, VecTrait,
    };
    use super::{BlockNumber, Epoch, EpochInfo, PublicKey, Staker, StakingPower};

    #[storage]
    struct Storage {
        stakers: Vec<Staker>,
        current_epoch: EpochInfo,
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

        fn set_current_epoch(ref self: ContractState, epoch: EpochInfo) {
            self.current_epoch.write(epoch);
        }

        // epoch_id is not used in this mock, but should be part of the interface.
        fn get_stakers(
            self: @ContractState, epoch_id: Epoch,
        ) -> Span<(ContractAddress, StakingPower, Option<PublicKey>)> {
            let mut stakers = array![];
            for i in 0..self.stakers.len() {
                let staker = self.stakers.at(i).read();
                stakers.append((staker.contract_address, staker.staking_power, staker.pub_key));
            }
            stakers.span()
        }

        fn get_current_epoch_data(self: @ContractState) -> (Epoch, BlockNumber, u32) {
            let epoch_info = self.current_epoch.read();
            (epoch_info.epoch_id, epoch_info.start_block, epoch_info.epoch_length)
        }
    }
}
