use apollo_test_utils::{get_rng, GetTestInstance};
use starknet_api::consensus_transaction::ConsensusTransaction;
use starknet_api::execution_resources::GasAmount;
use starknet_api::rpc_transaction::{
    RpcDeclareTransaction,
    RpcDeclareTransactionV3,
    RpcDeployAccountTransaction,
    RpcDeployAccountTransactionV3,
    RpcInvokeTransaction,
    RpcInvokeTransactionV3,
    RpcTransaction,
};

use crate::consensus::{
    ConsensusBlockInfo,
    ProposalFin,
    ProposalInit,
    ProposalPart,
    StreamMessage,
    StreamMessageBody,
    TransactionBatch,
    Vote,
};
use crate::converters::test_instances::TestStreamId;

// If all the fields of `AllResources` are 0 upon serialization,
// then the deserialized value will be interpreted as the `L1Gas` variant.
fn add_gas_values_to_transaction(transactions: &mut [ConsensusTransaction]) {
    let transaction = &mut transactions[0];
    match transaction {
        ConsensusTransaction::RpcTransaction(rpc_transaction) => match rpc_transaction {
            RpcTransaction::Declare(RpcDeclareTransaction::V3(RpcDeclareTransactionV3 {
                resource_bounds,
                ..
            }))
            | RpcTransaction::Invoke(RpcInvokeTransaction::V3(RpcInvokeTransactionV3 {
                resource_bounds,
                ..
            }))
            | RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(
                RpcDeployAccountTransactionV3 { resource_bounds, .. },
            )) => {
                resource_bounds.l2_gas.max_amount = GasAmount(1);
            }
        },
        ConsensusTransaction::L1Handler(_) => {}
    }
}

#[test]
fn convert_stream_message_to_vec_u8_and_back() {
    let mut rng = get_rng();

    // Test that we can convert a StreamMessage with a ProposalPart message to bytes and back.
    let mut stream_message: StreamMessage<ProposalPart, TestStreamId> =
        StreamMessage::get_test_instance(&mut rng);

    if let StreamMessageBody::Content(ProposalPart::Transactions(proposal)) =
        &mut stream_message.message
    {
        add_gas_values_to_transaction(&mut proposal.transactions);
    }

    let bytes_data: Vec<u8> = stream_message.clone().into();
    let res_data = StreamMessage::try_from(bytes_data).unwrap();
    assert_eq!(stream_message, res_data);
}

#[test]
fn convert_vote_to_vec_u8_and_back() {
    let mut rng = get_rng();

    let vote = Vote::get_test_instance(&mut rng);

    let bytes_data: Vec<u8> = vote.clone().into();
    let res_data = Vote::try_from(bytes_data).unwrap();
    assert_eq!(vote, res_data);
}

#[test]
fn convert_proposal_init_to_vec_u8_and_back() {
    let mut rng = get_rng();

    let proposal_init = ProposalInit::get_test_instance(&mut rng);

    let bytes_data: Vec<u8> = proposal_init.into();
    let res_data = ProposalInit::try_from(bytes_data).unwrap();
    assert_eq!(proposal_init, res_data);
}

#[test]
fn convert_block_info_to_vec_u8_and_back() {
    let mut rng = get_rng();

    let block_info = ConsensusBlockInfo::get_test_instance(&mut rng);

    let bytes_data: Vec<u8> = block_info.clone().into();
    let res_data = ConsensusBlockInfo::try_from(bytes_data).unwrap();
    assert_eq!(block_info, res_data);
}

#[test]
fn convert_transaction_batch_to_vec_u8_and_back() {
    let mut rng = get_rng();

    let mut transaction_batch = TransactionBatch::get_test_instance(&mut rng);

    add_gas_values_to_transaction(&mut transaction_batch.transactions);

    let bytes_data: Vec<u8> = transaction_batch.clone().into();
    let res_data = TransactionBatch::try_from(bytes_data).unwrap();
    assert_eq!(transaction_batch, res_data);
}

#[test]
fn convert_proposal_fin_to_vec_u8_and_back() {
    let mut rng = get_rng();

    let proposal_fin = ProposalFin::get_test_instance(&mut rng);

    let bytes_data: Vec<u8> = proposal_fin.clone().into();
    let res_data = ProposalFin::try_from(bytes_data).unwrap();
    assert_eq!(proposal_fin, res_data);
}

#[test]
fn convert_proposal_part_to_vec_u8_and_back() {
    let mut rng = get_rng();

    let mut proposal_part = ProposalPart::get_test_instance(&mut rng);

    if let ProposalPart::Transactions(ref mut transaction_batch) = proposal_part {
        add_gas_values_to_transaction(&mut transaction_batch.transactions);
    }

    let bytes_data: Vec<u8> = proposal_part.clone().into();
    let res_data = ProposalPart::try_from(bytes_data).unwrap();
    assert_eq!(proposal_part, res_data);
}

#[test]
fn stream_message_display() {
    let mut rng = get_rng();
    let stream_id = TestStreamId(42);
    let message_id = 127;
    let proposal = ProposalPart::get_test_instance(&mut rng);
    let proposal_bytes: Vec<u8> = proposal.clone().into();
    let proposal_length = proposal_bytes.len();
    let content = StreamMessageBody::Content(proposal);
    let message = StreamMessage { message: content, stream_id, message_id };

    let txt = message.to_string();
    assert_eq!(
        txt,
        format!(
            "StreamMessage {{ stream_id: {stream_id}, message_id: {message_id}, message_length: \
             {proposal_length}}}"
        )
    );

    let content: StreamMessageBody<ProposalPart> = StreamMessageBody::Fin;
    let message = StreamMessage { message: content, stream_id, message_id };
    let txt = message.to_string();
    assert_eq!(
        txt,
        format!(
            "StreamMessage {{ stream_id: {stream_id}, message_id: {message_id}, message is fin }}"
        )
    );
}
