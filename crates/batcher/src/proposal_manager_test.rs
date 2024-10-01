use std::ops::Range;
use std::sync::Arc;
use std::vec;

use assert_matches::assert_matches;
use async_trait::async_trait;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use futures::future::BoxFuture;
use futures::FutureExt;
#[cfg(test)]
use mockall::automock;
use rstest::{fixture, rstest};
use starknet_api::block::BlockNumber;
use starknet_api::core::ClassHash;
use starknet_api::executable_transaction::Transaction;
use starknet_api::state::ThinStateDiff;
use starknet_api::test_utils::invoke::{executable_invoke_tx, InvokeTxArgs};
use starknet_api::transaction::TransactionHash;
use starknet_api::{class_hash, felt};
use starknet_batcher_types::batcher_types::{GetProposalContent, ProposalCommitment, ProposalId};
use starknet_mempool_types::communication::MockMempoolClient;
use tokio::sync::Mutex;
use tokio_stream::StreamExt;

use crate::batcher::{MockBatcherStorageReaderTrait, MockBatcherStorageWriterTrait};
use crate::proposal_manager::{
    BlockBuilderOutput,
    BlockBuilderTrait,
    BuildProposalError,
    GetProposalContentError,
    InputTxStream,
    MockBlockBuilderFactoryTrait,
    ProposalContentStream,
    ProposalManager,
    ProposalManagerConfig,
    StartHeightError,
};

const INITIAL_HEIGHT: BlockNumber = BlockNumber(3);
const STREAMING_CHUNK_SIZE: usize = 3;

#[fixture]
fn proposal_manager_config() -> ProposalManagerConfig {
    ProposalManagerConfig::default()
}

// TODO: Figure out how to pass expectations to the mock.
#[fixture]
fn block_builder_factory() -> MockBlockBuilderFactoryTrait {
    MockBlockBuilderFactoryTrait::new()
}

// TODO: Figure out how to pass expectations to the mock.
#[fixture]
fn mempool_client() -> MockMempoolClient {
    MockMempoolClient::new()
}

#[fixture]
fn output_streaming() -> (tokio::sync::mpsc::Sender<GetProposalContent>, ProposalContentStream) {
    const OUTPUT_CONTENT_BUFFER_SIZE: usize = 100;
    let (output_content_sender, output_content_receiver) =
        tokio::sync::mpsc::channel(OUTPUT_CONTENT_BUFFER_SIZE);
    let stream = ProposalContentStream::BuildProposal(tokio_stream::wrappers::ReceiverStream::new(
        output_content_receiver,
    ));
    (output_content_sender, stream)
}

type CurrentHeight = Arc<Mutex<BlockNumber>>;

#[fixture]
fn storage() -> (MockBatcherStorageReaderTrait, MockBatcherStorageWriterTrait, CurrentHeight) {
    let current_height = Arc::new(Mutex::new(INITIAL_HEIGHT));
    let cloned = current_height.clone();
    let mut storage_reader = MockBatcherStorageReaderTrait::new();
    storage_reader.expect_height().returning(move || Ok(*cloned.try_lock().unwrap()));
    let storage_writer = MockBatcherStorageWriterTrait::new();
    (storage_reader, storage_writer, current_height)
}

#[fixture]
fn proposal_manager(
    proposal_manager_config: ProposalManagerConfig,
    mempool_client: MockMempoolClient,
    block_builder_factory: MockBlockBuilderFactoryTrait,
    storage: (MockBatcherStorageReaderTrait, MockBatcherStorageWriterTrait, CurrentHeight),
) -> ProposalManager {
    ProposalManager::new(
        proposal_manager_config,
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
        Arc::new(storage.0),
        Box::new(storage.1),
    )
}

#[rstest]
#[case::height_already_passed(
    INITIAL_HEIGHT.prev().unwrap(),
    Result::Err(StartHeightError::HeightAlreadyPassed {
        storage_height: INITIAL_HEIGHT,
        requested_height: INITIAL_HEIGHT.prev().unwrap()
    }
))]
#[case::happy(
    INITIAL_HEIGHT,
    Result::Ok(())
)]
#[case::storage_not_synced(
    INITIAL_HEIGHT.unchecked_next(),
    Result::Err(StartHeightError::StorageNotSynced {
        storage_height: INITIAL_HEIGHT,
        requested_height: INITIAL_HEIGHT.unchecked_next()
    }
))]
fn start_height(
    mut proposal_manager: ProposalManager,
    #[case] height: BlockNumber,
    #[case] expected_result: Result<(), StartHeightError>,
) {
    let result = proposal_manager.start_height(height);
    // Unfortunatelly ProposalManagerError doesn't implement PartialEq.
    assert_eq!(format!("{:?}", result), format!("{:?}", expected_result));
}

#[rstest]
#[tokio::test]
async fn proposal_generation_fails_without_start_height(mut proposal_manager: ProposalManager) {
    let err = proposal_manager.build_block_proposal(ProposalId(0), arbitrary_deadline()).await;
    assert_matches!(err, Err(BuildProposalError::NoActiveHeight));
}

#[rstest]
#[tokio::test]
async fn proposal_generation_success(
    proposal_manager_config: ProposalManagerConfig,
    mut block_builder_factory: MockBlockBuilderFactoryTrait,
    mut mempool_client: MockMempoolClient,
    storage: (MockBatcherStorageReaderTrait, MockBatcherStorageWriterTrait, CurrentHeight),
) {
    let n_txs = 2 * proposal_manager_config.max_txs_per_mempool_request;
    block_builder_factory
        .expect_create_block_builder()
        .once()
        .returning(move || simulate_build_block(Some(n_txs), BlockBuilderOutput::default()));

    mempool_client.expect_get_txs().once().returning(|max_n_txs| Ok(test_txs(0..max_n_txs)));

    mempool_client
        .expect_get_txs()
        .once()
        .returning(|max_n_txs| Ok(test_txs(max_n_txs..2 * max_n_txs)));

    mempool_client.expect_get_txs().returning(|_| Ok(vec![]));

    let mut proposal_manager = ProposalManager::new(
        proposal_manager_config.clone(),
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
        Arc::new(storage.0),
        Box::new(storage.1),
    );

    proposal_manager.start_height(INITIAL_HEIGHT).unwrap();

    let proposal_id = ProposalId(0);
    proposal_manager.build_block_proposal(proposal_id, arbitrary_deadline()).await.unwrap();

    await_proposal(proposal_id, &mut proposal_manager).await;
}

#[rstest]
#[tokio::test]
async fn consecutive_proposal_generations_success(
    proposal_manager_config: ProposalManagerConfig,
    mut block_builder_factory: MockBlockBuilderFactoryTrait,
    mut mempool_client: MockMempoolClient,
    storage: (MockBatcherStorageReaderTrait, MockBatcherStorageWriterTrait, CurrentHeight),
) {
    let n_txs = proposal_manager_config.max_txs_per_mempool_request;
    block_builder_factory
        .expect_create_block_builder()
        .times(2)
        .returning(move || simulate_build_block(Some(n_txs), BlockBuilderOutput::default()));

    let expected_txs = test_txs(0..proposal_manager_config.max_txs_per_mempool_request);
    let mempool_txs = expected_txs.clone();
    mempool_client.expect_get_txs().returning(move |_max_n_txs| Ok(mempool_txs.clone()));

    let mut proposal_manager = ProposalManager::new(
        proposal_manager_config.clone(),
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
        Arc::new(storage.0),
        Box::new(storage.1),
    );

    proposal_manager.start_height(INITIAL_HEIGHT).unwrap();

    proposal_manager.build_block_proposal(ProposalId(0), arbitrary_deadline()).await.unwrap();

    // Make sure the first proposal generated successfully.
    await_proposal(ProposalId(0), &mut proposal_manager).await;

    proposal_manager.build_block_proposal(ProposalId(1), arbitrary_deadline()).await.unwrap();

    // Make sure the proposal generated successfully.
    await_proposal(ProposalId(1), &mut proposal_manager).await;
}

#[rstest]
#[tokio::test]
async fn multiple_proposals_generation_fail(
    proposal_manager_config: ProposalManagerConfig,
    mut block_builder_factory: MockBlockBuilderFactoryTrait,
    mut mempool_client: MockMempoolClient,
    storage: (MockBatcherStorageReaderTrait, MockBatcherStorageWriterTrait, CurrentHeight),
) {
    // The block builder will never stop.
    block_builder_factory
        .expect_create_block_builder()
        .once()
        .returning(|| simulate_build_block(None, BlockBuilderOutput::default()));

    mempool_client.expect_get_txs().returning(|_| Ok(vec![]));

    let mut proposal_manager = ProposalManager::new(
        proposal_manager_config.clone(),
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
        Arc::new(storage.0),
        Box::new(storage.1),
    );

    proposal_manager.start_height(INITIAL_HEIGHT).unwrap();

    // A proposal that will never finish.
    proposal_manager.build_block_proposal(ProposalId(0), arbitrary_deadline()).await.unwrap();

    // Try to generate another proposal while the first one is still being generated.
    let another_generate_request =
        proposal_manager.build_block_proposal(ProposalId(1), arbitrary_deadline()).await;
    assert_matches!(
        another_generate_request,
        Err(BuildProposalError::AlreadyGeneratingProposal {
            current_generating_proposal_id,
            new_proposal_id
        }) if current_generating_proposal_id == ProposalId(0) && new_proposal_id == ProposalId(1)
    );
}

#[rstest]
#[tokio::test]
async fn get_stream_content(
    proposal_manager_config: ProposalManagerConfig,
    mut block_builder_factory: MockBlockBuilderFactoryTrait,
    mut mempool_client: MockMempoolClient,
    storage: (MockBatcherStorageReaderTrait, MockBatcherStorageWriterTrait, CurrentHeight),
) {
    const PROPOSAL_ID: ProposalId = ProposalId(0);
    let n_txs = 2 * proposal_manager_config.max_txs_per_mempool_request;
    block_builder_factory
        .expect_create_block_builder()
        .once()
        .returning(move || simulate_build_block(Some(n_txs), BlockBuilderOutput::default()));

    mempool_client.expect_get_txs().once().returning(|max_n_txs| Ok(test_txs(0..max_n_txs)));

    mempool_client
        .expect_get_txs()
        .once()
        .returning(|max_n_txs| Ok(test_txs(max_n_txs..2 * max_n_txs)));

    mempool_client.expect_get_txs().returning(|_| Ok(vec![]));

    let mut proposal_manager = ProposalManager::new(
        proposal_manager_config.clone(),
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
        Arc::new(storage.0),
        Box::new(storage.1),
    );

    proposal_manager.start_height(INITIAL_HEIGHT).unwrap();

    proposal_manager.build_block_proposal(PROPOSAL_ID, arbitrary_deadline()).await.unwrap();

    let n_chunks = n_txs.div_ceil(STREAMING_CHUNK_SIZE);
    for _ in 0..n_chunks {
        let content = proposal_manager.get_proposal_content(PROPOSAL_ID).await;
        assert_matches!(content, Ok(GetProposalContent::Txs(_)));
    }
    let finished = proposal_manager.get_proposal_content(PROPOSAL_ID).await;
    assert_matches!(finished, Ok(GetProposalContent::Finished(_)));
    let exhausted = proposal_manager.get_proposal_content(PROPOSAL_ID).await;
    assert_matches!(exhausted, Err(GetProposalContentError::StreamExhausted));
}

#[rstest]
#[tokio::test]
async fn decision_reached_no_active_proposal(
    proposal_manager_config: ProposalManagerConfig,
    mut block_builder_factory: MockBlockBuilderFactoryTrait,
    mut mempool_client: MockMempoolClient,
    storage: (MockBatcherStorageReaderTrait, MockBatcherStorageWriterTrait, CurrentHeight),
) {
    let n_txs = proposal_manager_config.max_txs_per_mempool_request;
    let proposal_0_output = BlockBuilderOutput::Done {
        state_diff: ThinStateDiff::default(),
        casms: vec![(class_hash!("0x0"), CasmContractClass::default())],
    };
    let proposal_1_output = BlockBuilderOutput::Done {
        state_diff: ThinStateDiff::default(),
        casms: vec![(class_hash!("0x1"), CasmContractClass::default())],
    };
    block_builder_factory
        .expect_create_block_builder()
        .once()
        .returning(move || simulate_build_block(Some(n_txs), proposal_0_output.clone()));

    block_builder_factory
        .expect_create_block_builder()
        .once()
        .returning(move || simulate_build_block(Some(n_txs), proposal_1_output.clone()));

    let expected_txs = test_txs(0..proposal_manager_config.max_txs_per_mempool_request);
    let mempool_txs = expected_txs.clone();
    mempool_client.expect_get_txs().returning(move |_max_n_txs| Ok(mempool_txs.clone()));

    let (storage_reader, mut storage_writer, current_height) = storage;

    storage_writer
        .expect_commit_proposal()
        .once()
        .withf(|height, _state_diff, casms| {
            *height == INITIAL_HEIGHT && casms.first().unwrap().0 == class_hash!("0x0")
        })
        .returning(move |_, _, _| {
            advance_current_height(&current_height);
            Ok(())
        });

    let mut proposal_manager = ProposalManager::new(
        proposal_manager_config.clone(),
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
        Arc::new(storage_reader),
        Box::new(storage_writer),
    );

    proposal_manager.start_height(INITIAL_HEIGHT).unwrap();

    proposal_manager.build_block_proposal(ProposalId(0), arbitrary_deadline()).await.unwrap();

    // Make sure the first proposal generated successfully.
    await_proposal(ProposalId(0), &mut proposal_manager).await;

    proposal_manager.build_block_proposal(ProposalId(1), arbitrary_deadline()).await.unwrap();

    // Make sure the proposal generated successfully.
    await_proposal(ProposalId(1), &mut proposal_manager).await;

    proposal_manager.decision_reached(ProposalId(0)).await.unwrap();

    // Now start height should pass.
    proposal_manager.start_height(INITIAL_HEIGHT.unchecked_next()).unwrap();
}

#[rstest]
#[tokio::test]
async fn decision_reached_aborting_active_proposal(
    proposal_manager_config: ProposalManagerConfig,
    mut block_builder_factory: MockBlockBuilderFactoryTrait,
    mut mempool_client: MockMempoolClient,
    storage: (MockBatcherStorageReaderTrait, MockBatcherStorageWriterTrait, CurrentHeight),
) {
    let n_txs = proposal_manager_config.max_txs_per_mempool_request;
    let proposal_0_output = BlockBuilderOutput::Done {
        state_diff: ThinStateDiff::default(),
        casms: vec![(class_hash!("0x0"), CasmContractClass::default())],
    };

    block_builder_factory
        .expect_create_block_builder()
        .once()
        .returning(move || simulate_build_block(Some(n_txs), proposal_0_output.clone()));

    block_builder_factory
        .expect_create_block_builder()
        .once()
        .returning(move || simulate_aborted_build_block(ProposalId(1)));

    let expected_txs = test_txs(0..proposal_manager_config.max_txs_per_mempool_request);
    let mempool_txs = expected_txs.clone();
    mempool_client.expect_get_txs().returning(move |_max_n_txs| Ok(mempool_txs.clone()));

    let (storage_reader, mut storage_writer, current_height) = storage;

    storage_writer
        .expect_commit_proposal()
        .once()
        .withf(|height, _state_diff, casms| {
            *height == INITIAL_HEIGHT && casms.first().unwrap().0 == class_hash!("0x0")
        })
        .returning(move |_, _, _| {
            advance_current_height(&current_height);
            Ok(())
        });

    let mut proposal_manager = ProposalManager::new(
        proposal_manager_config.clone(),
        Arc::new(mempool_client),
        Arc::new(block_builder_factory),
        Arc::new(storage_reader),
        Box::new(storage_writer),
    );

    proposal_manager.start_height(INITIAL_HEIGHT).unwrap();

    proposal_manager.build_block_proposal(ProposalId(0), arbitrary_deadline()).await.unwrap();

    // Make sure the first proposal generated successfully.
    await_proposal(ProposalId(0), &mut proposal_manager).await;

    // This proposal should be aborted.
    proposal_manager.build_block_proposal(ProposalId(1), arbitrary_deadline()).await.unwrap();

    proposal_manager.decision_reached(ProposalId(0)).await.unwrap();

    // Now start height should pass.
    proposal_manager.start_height(INITIAL_HEIGHT.unchecked_next()).unwrap();
}

fn arbitrary_deadline() -> tokio::time::Instant {
    const GENERATION_TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_secs(1);
    tokio::time::Instant::now() + GENERATION_TIMEOUT
}

fn test_txs(tx_hash_range: Range<usize>) -> Vec<Transaction> {
    tx_hash_range
        .map(|i| {
            Transaction::Invoke(executable_invoke_tx(InvokeTxArgs {
                tx_hash: TransactionHash(felt!(u128::try_from(i).unwrap())),
                ..Default::default()
            }))
        })
        .collect()
}

fn simulate_build_block(
    n_txs: Option<usize>,
    returning: BlockBuilderOutput,
) -> Arc<dyn BlockBuilderTrait> {
    let mut mock_block_builder = MockBlockBuilderTraitWrapper::new();
    mock_block_builder.expect_build_block().return_once(
        move |deadline, mempool_tx_stream, output_content_sender| {
            simulate_block_builder(
                deadline,
                mempool_tx_stream,
                output_content_sender,
                n_txs,
                returning,
            )
            .boxed()
        },
    );
    Arc::new(mock_block_builder)
}

fn simulate_aborted_build_block(proposal_id: ProposalId) -> Arc<dyn BlockBuilderTrait> {
    let mut mock_block_builder = MockBlockBuilderTraitWrapper::new();
    mock_block_builder.expect_build_block().return_once(
        move |deadline, mempool_tx_stream, output_content_sender| {
            simulate_block_builder(
                deadline,
                mempool_tx_stream,
                output_content_sender,
                None,
                BlockBuilderOutput::default(),
            )
            .boxed()
        },
    );
    mock_block_builder.expect_abort_build().return_once(move || {
        println!(
            "<<TEST LOG:`block_builder.abort_build()` has been called for proposal {}.",
            proposal_id
        );
        BlockBuilderOutput::Aborted
    });
    Arc::new(mock_block_builder)
}

async fn simulate_block_builder(
    _deadline: tokio::time::Instant,
    mempool_tx_stream: InputTxStream,
    output_sender: tokio::sync::mpsc::Sender<GetProposalContent>,
    n_txs_to_take: Option<usize>,
    returning: BlockBuilderOutput,
) -> BlockBuilderOutput {
    let mut mempool_tx_stream = mempool_tx_stream.take(n_txs_to_take.unwrap_or(usize::MAX));
    let mut to_stream = vec![];
    while let Some(tx) = mempool_tx_stream.next().await {
        to_stream.push(tx);
    }
    let streaming_chunks = to_stream.chunks(STREAMING_CHUNK_SIZE);
    for chunk in streaming_chunks {
        let content = GetProposalContent::Txs(chunk.to_vec());
        output_sender.send(content).await.unwrap();
    }
    output_sender.send(GetProposalContent::Finished(ProposalCommitment::default())).await.unwrap();
    returning
}

// A wrapper trait to allow mocking the BlockBuilderTrait in tests.
#[cfg_attr(test, automock)]
trait BlockBuilderTraitWrapper: Send + Sync {
    // Equivalent to: async fn build_block(&self, deadline: tokio::time::Instant);
    fn build_block(
        &self,
        deadline: tokio::time::Instant,
        tx_stream: InputTxStream,
        output_content_sender: tokio::sync::mpsc::Sender<GetProposalContent>,
    ) -> BoxFuture<'_, BlockBuilderOutput>;

    fn abort_build(&self) -> BlockBuilderOutput;
}

#[async_trait]
impl<T: BlockBuilderTraitWrapper> BlockBuilderTrait for T {
    async fn build_block(
        &self,
        deadline: tokio::time::Instant,
        tx_stream: InputTxStream,
        output_content_sender: tokio::sync::mpsc::Sender<GetProposalContent>,
    ) -> BlockBuilderOutput {
        self.build_block(deadline, tx_stream, output_content_sender).await
    }

    fn abort_build(&self) -> BlockBuilderOutput {
        self.abort_build()
    }
}

fn advance_current_height(current_height: &Arc<Mutex<BlockNumber>>) {
    println!("<<TEST LOG>>: Advancing current height.");
    let mut current_height =
        current_height.try_lock().expect("Current height mutex is already locked.");
    *current_height = current_height.unchecked_next();
}

async fn await_proposal(proposal_id: ProposalId, proposal_manager: &mut ProposalManager) {
    loop {
        if let GetProposalContent::Finished(_) =
            proposal_manager.get_proposal_content(proposal_id).await.unwrap()
        {
            break;
        }
    }
}
