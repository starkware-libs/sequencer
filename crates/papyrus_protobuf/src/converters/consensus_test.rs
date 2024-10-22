use papyrus_test_utils::{
    auto_impl_get_test_instance,
    get_number_of_variants,
    get_rng,
    GetTestInstance,
};
use rand::Rng;
use starknet_api::block::BlockHash;
use starknet_api::core::ContractAddress;
use starknet_api::execution_resources::GasAmount;
use starknet_api::transaction::fields::ValidResourceBounds;
use starknet_api::transaction::{
    DeclareTransaction,
    DeclareTransactionV3,
    DeployAccountTransaction,
    DeployAccountTransactionV3,
    InvokeTransaction,
    InvokeTransactionV3,
    Transaction,
};

use crate::consensus::{
    ConsensusMessage,
    Proposal,
    StreamMessage,
    StreamMessageBody,
    Vote,
    VoteType,
};

// If all the fields of `AllResources` are 0 upon serialization,
// then the deserialized value will be interpreted as the `L1Gas` variant.
fn add_gas_values_to_transaction(transactions: &mut Vec<Transaction>) {
    let transaction = &mut transactions[0];
    match transaction {
        Transaction::Declare(DeclareTransaction::V3(DeclareTransactionV3 {
            resource_bounds,
            ..
        }))
        | Transaction::Invoke(InvokeTransaction::V3(InvokeTransactionV3 {
            resource_bounds, ..
        }))
        | Transaction::DeployAccount(DeployAccountTransaction::V3(DeployAccountTransactionV3 {
            resource_bounds,
            ..
        })) => {
            if let ValidResourceBounds::AllResources(ref mut bounds) = resource_bounds {
                bounds.l2_gas.max_amount = GasAmount(1);
            }
        }
        _ => {}
    }
}

auto_impl_get_test_instance! {
    pub enum ConsensusMessage {
        Proposal(Proposal) = 0,
        Vote(Vote) = 1,
    }
}

auto_impl_get_test_instance! {
    pub struct Proposal {
        pub height: u64,
        pub round: u32,
        pub proposer: ContractAddress,
        pub transactions: Vec<Transaction>,
        pub block_hash: BlockHash,
        pub valid_round: Option<u32>,
    }
}

auto_impl_get_test_instance! {
    pub struct Vote {
        pub vote_type: VoteType,
        pub height: u64,
        pub round: u32,
        pub block_hash: Option<BlockHash>,
        pub voter: ContractAddress,
    }
}

auto_impl_get_test_instance! {
    pub enum VoteType {
        Prevote = 0,
        Precommit = 1,
    }
}

// The auto_impl_get_test_instance macro does not work for StreamMessage because it has
// a generic type. TODO(guyn): try to make the macro work with generic types.
impl GetTestInstance for StreamMessage<ConsensusMessage> {
    fn get_test_instance(rng: &mut rand_chacha::ChaCha8Rng) -> Self {
        let message = if rng.gen_bool(0.5) {
            StreamMessageBody::Content(ConsensusMessage::Proposal(Proposal::get_test_instance(rng)))
        } else {
            StreamMessageBody::Fin
        };
        Self { message, stream_id: rng.gen_range(0..100), message_id: rng.gen_range(0..1000) }
    }
}

#[test]
fn convert_stream_message_to_vec_u8_and_back() {
    let mut rng = get_rng();

    // Test that we can convert a StreamMessage with a ConsensusMessage message to bytes and back.
    let mut stream_message: StreamMessage<ConsensusMessage> =
        StreamMessage::get_test_instance(&mut rng);

    if let StreamMessageBody::Content(ConsensusMessage::Proposal(proposal)) =
        &mut stream_message.message
    {
        add_gas_values_to_transaction(&mut proposal.transactions);
    }

    let bytes_data: Vec<u8> = stream_message.clone().into();
    let res_data = StreamMessage::try_from(bytes_data).unwrap();
    assert_eq!(stream_message, res_data);
}

#[test]
fn convert_consensus_message_to_vec_u8_and_back() {
    let mut rng = get_rng();

    // Test that we can convert a ConsensusMessage to bytes and back.
    let mut message = ConsensusMessage::get_test_instance(&mut rng);

    if let ConsensusMessage::Proposal(proposal) = &mut message {
        add_gas_values_to_transaction(&mut proposal.transactions);
    }

    let bytes_data: Vec<u8> = message.clone().into();
    let res_data = ConsensusMessage::try_from(bytes_data).unwrap();
    assert_eq!(message, res_data);
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
fn convert_proposal_to_vec_u8_and_back() {
    let mut rng = get_rng();

    let mut proposal = Proposal::get_test_instance(&mut rng);

    add_gas_values_to_transaction(&mut proposal.transactions);

    let bytes_data: Vec<u8> = proposal.clone().into();
    let res_data = Proposal::try_from(bytes_data).unwrap();
    assert_eq!(proposal, res_data);
}

#[test]
fn stream_message_display() {
    let mut rng = get_rng();
    let stream_id = 42;
    let message_id = 127;
    let proposal = Proposal::get_test_instance(&mut rng);
    let proposal_bytes: Vec<u8> = proposal.clone().into();
    let proposal_length = proposal_bytes.len();
    let content = StreamMessageBody::Content(proposal);
    let message = StreamMessage { message: content, stream_id, message_id };

    let txt = message.to_string();
    assert_eq!(
        txt,
        format!(
            "StreamMessage {{ stream_id: {}, message_id: {}, message_length: {}}}",
            stream_id, message_id, proposal_length
        )
    );

    let content: StreamMessageBody<Proposal> = StreamMessageBody::Fin;
    let message = StreamMessage { message: content, stream_id, message_id };
    let txt = message.to_string();
    assert_eq!(
        txt,
        format!(
            "StreamMessage {{ stream_id: {}, message_id: {}, message is fin }}",
            stream_id, message_id
        )
    );
}
