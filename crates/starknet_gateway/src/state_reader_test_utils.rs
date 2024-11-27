use blockifier::blockifier::block::BlockInfo;
use blockifier::context::BlockContext;
use blockifier::execution::contract_class::RunnableCompiledClass;
use blockifier::state::errors::StateError;
use blockifier::state::state_api::{StateReader as BlockifierStateReader, StateResult};
use blockifier::test_utils::contracts::FeatureContract;
use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier::test_utils::initial_test_state::test_state;
use blockifier::test_utils::{CairoVersion, BALANCE};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_api::transaction::fields::Fee;
use starknet_types_core::felt::Felt;

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
    ) -> StateResult<Felt> {
        self.blockifier_state_reader.get_storage_at(contract_address, key)
    }

    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        self.blockifier_state_reader.get_nonce_at(contract_address)
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        self.blockifier_state_reader.get_class_hash_at(contract_address)
    }

    fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        self.blockifier_state_reader.get_compiled_class(class_hash)
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

pub fn local_test_state_reader_factory(
    cairo_version: CairoVersion,
    zero_balance: bool,
) -> TestStateReaderFactory {
    let block_context = BlockContext::create_for_testing();
    let account_balance = if zero_balance { Fee(0) } else { BALANCE };
    let account_contract = FeatureContract::AccountWithoutValidations(cairo_version);
    let test_contract = FeatureContract::TestContract(cairo_version);

    let state_reader = test_state(
        block_context.chain_info(),
        account_balance,
        &[(account_contract, 1), (test_contract, 1)],
    );

    TestStateReaderFactory {
        state_reader: TestStateReader {
            block_info: block_context.block_info().clone(),
            blockifier_state_reader: state_reader.state,
        },
    }
}
