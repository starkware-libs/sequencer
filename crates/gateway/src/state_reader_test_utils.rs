use assert_matches::assert_matches;
use blockifier::blockifier::block::BlockInfo;
use blockifier::context::{BlockContext, ChainInfo};
use blockifier::execution::contract_class::ContractClass;
use blockifier::state::errors::StateError;
use blockifier::state::state_api::{StateReader as BlockifierStateReader, StateResult};
use blockifier::test_utils::contracts::FeatureContract;
use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier::test_utils::initial_test_state::{fund_account, test_state_reader};
use blockifier::test_utils::{CairoVersion, BALANCE};
use blockifier::versioned_constants::VersionedConstants;
use starknet_api::block::BlockNumber;
use starknet_api::core::{
    calculate_contract_address, ClassHash, CompiledClassHash, ContractAddress, Nonce,
};
use starknet_api::hash::StarkFelt;
use starknet_api::rpc_transaction::{RPCDeployAccountTransaction, RPCTransaction};
use starknet_api::state::StorageKey;

use crate::state_reader::{MempoolStateReader, StateReaderFactory};

#[derive(Clone)]
pub struct TestStateReader {
    pub block_info: BlockInfo,
    pub blockifier_state_reader: DictStateReader,
}

impl MempoolStateReader for TestStateReader {
    fn get_block_info(&self) -> Result<BlockInfo, StateError> {
        Ok(self.block_info.clone())
    }
}

impl BlockifierStateReader for TestStateReader {
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<StarkFelt> {
        self.blockifier_state_reader.get_storage_at(contract_address, key)
    }

    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        self.blockifier_state_reader.get_nonce_at(contract_address)
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        self.blockifier_state_reader.get_class_hash_at(contract_address)
    }

    fn get_compiled_contract_class(&self, class_hash: ClassHash) -> StateResult<ContractClass> {
        self.blockifier_state_reader.get_compiled_contract_class(class_hash)
    }

    fn get_compiled_class_hash(&self, class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        self.blockifier_state_reader.get_compiled_class_hash(class_hash)
    }
}

pub struct TestStateReaderFactory {
    pub state_reader: TestStateReader,
}

impl StateReaderFactory for TestStateReaderFactory {
    fn get_state_reader_from_latest_block(&self) -> Box<dyn MempoolStateReader> {
        Box::new(self.state_reader.clone())
    }

    fn get_state_reader(&self, _block_number: BlockNumber) -> Box<dyn MempoolStateReader> {
        Box::new(self.state_reader.clone())
    }
}

pub fn local_test_state_reader_factory(zero_balance: bool) -> TestStateReaderFactory {
    let account_balance = if zero_balance { 0 } else { BALANCE };
    let cairo_version = CairoVersion::Cairo1;
    let block_context = BlockContext::new_unchecked(
        &BlockInfo::create_for_testing(),
        &ChainInfo::create_for_testing(),
        &VersionedConstants::create_for_testing(),
    );
    let account_contract = FeatureContract::AccountWithoutValidations(cairo_version);
    let test_contract = FeatureContract::TestContract(cairo_version);

    let state_reader = test_state_reader(
        block_context.chain_info(),
        account_balance,
        &[(account_contract, 1), (test_contract, 1)],
    );

    TestStateReaderFactory {
        state_reader: TestStateReader {
            block_info: block_context.block_info().clone(),
            blockifier_state_reader: state_reader,
        },
    }
}

pub fn local_test_state_reader_factory_for_deploy_account(
    deploy_tx: &RPCTransaction,
) -> TestStateReaderFactory {
    let mut state_reader_factory = local_test_state_reader_factory(false);

    // Fund the deployed_account_address.
    let tx = assert_matches!(
        deploy_tx,
        RPCTransaction::DeployAccount(RPCDeployAccountTransaction::V3(tx)) => tx
    );

    let deployed_account_address = calculate_contract_address(
        tx.contract_address_salt,
        tx.class_hash,
        &tx.constructor_calldata,
        ContractAddress::default(),
    )
    .unwrap();
    fund_account(
        BlockContext::create_for_testing().chain_info(),
        deployed_account_address,
        BALANCE,
        &mut state_reader_factory.state_reader.blockifier_state_reader,
    );
    state_reader_factory
}
