use apollo_state_sync_types::communication::StateSyncClientResult;
use async_trait::async_trait;
use blockifier::context::BlockContext;
use blockifier::execution::contract_class::RunnableCompiledClass;
use blockifier::state::global_cache::CompiledClasses;
use blockifier::state::state_api::{StateReader as BlockifierStateReader, StateResult};
use blockifier::state::state_reader_and_contract_manager::FetchCompiledClasses;
use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier::test_utils::initial_test_state::test_state;
use blockifier_test_utils::cairo_versions::CairoVersion;
use blockifier_test_utils::contracts::FeatureContract;
use starknet_api::block::BlockInfo;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_api::test_utils::VALID_ACCOUNT_BALANCE;
use starknet_api::transaction::fields::Fee;
use starknet_types_core::felt::Felt;

use crate::gateway_fixed_block_state_reader::{GatewayFixedBlockStateReader, StarknetResult};
use crate::state_reader::{GatewayStateReaderWithCompiledClasses, StateReaderFactory};

// TODO(Itamar): Consider removing this struct.
#[derive(Clone)]
pub struct TestStateReader {
    pub blockifier_state_reader: DictStateReader,
}

impl FetchCompiledClasses for TestStateReader {
    fn get_compiled_classes(&self, class_hash: ClassHash) -> StateResult<CompiledClasses> {
        self.blockifier_state_reader.get_compiled_classes(class_hash)
    }

    fn is_declared(&self, class_hash: ClassHash) -> StateResult<bool> {
        self.blockifier_state_reader.is_declared(class_hash)
    }
}

impl GatewayStateReaderWithCompiledClasses for TestStateReader {}

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

#[derive(Clone)]
pub struct TestGatewayFixedBlockStateReader {
    pub block_info: BlockInfo,
}

impl TestGatewayFixedBlockStateReader {
    pub fn new(block_info: BlockInfo) -> Self {
        Self { block_info }
    }
}

#[async_trait]
impl GatewayFixedBlockStateReader for TestGatewayFixedBlockStateReader {
    async fn get_block_info(&self) -> StarknetResult<BlockInfo> {
        Ok(self.block_info.clone())
    }

    async fn get_nonce(&self, _contract_address: ContractAddress) -> StarknetResult<Nonce> {
        Ok(Nonce::default())
    }
}

pub struct TestStateReaderFactory {
    pub state_reader: TestStateReader,
    pub gateway_fixed_block: TestGatewayFixedBlockStateReader,
}

#[async_trait]
impl StateReaderFactory for TestStateReaderFactory {
    type StateReaderWithCompiledClasses = TestStateReader;
    type FixedBlockStateReader = TestGatewayFixedBlockStateReader;

    async fn get_blockifier_state_reader_and_gateway_fixed_block_from_latest_block(
        &self,
    ) -> StateSyncClientResult<(Self::StateReaderWithCompiledClasses, Self::FixedBlockStateReader)>
    {
        Ok((self.state_reader.clone(), self.gateway_fixed_block.clone()))
    }
}

pub fn local_test_state_reader_factory(
    cairo_version: CairoVersion,
    zero_balance: bool,
) -> TestStateReaderFactory {
    let block_context = BlockContext::create_for_testing();
    let account_balance = if zero_balance { Fee(0) } else { VALID_ACCOUNT_BALANCE };
    let account_contract = FeatureContract::AccountWithoutValidations(cairo_version);
    let test_contract = FeatureContract::TestContract(cairo_version);

    let state_reader = test_state(
        block_context.chain_info(),
        account_balance,
        &[(account_contract, 1), (test_contract, 1)],
    );

    TestStateReaderFactory {
        state_reader: TestStateReader { blockifier_state_reader: state_reader.state },
        gateway_fixed_block: TestGatewayFixedBlockStateReader::new(
            block_context.block_info().clone(),
        ),
    }
}
