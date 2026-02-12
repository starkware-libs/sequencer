use std::sync::{Arc, Mutex};

use apollo_transaction_converter::transaction_converter::verify_proof;
use async_trait::async_trait;
use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use starknet_api::core::EthAddress;
use starknet_api::test_utils::invoke::rpc_invoke_tx;
use starknet_api::transaction::{InvokeTransaction, L2ToL1Payload, MessageToL1};
use starknet_api::{calldata, invoke_tx_args};
use starknet_os_runner::errors::RunnerError;
use starknet_os_runner::runner::{RunnerOutput, VirtualSnosRunner};
use starknet_os_runner::virtual_snos_prover::{VirtualSnosProver, VirtualSnosProverOutput};
use starknet_types_core::felt::Felt;

use crate::initial_state::FlowTestState;
use crate::test_manager::{TestBuilder, TestRunner};

/// A runner that returns pre-computed data from a previous virtual OS execution.
#[derive(Clone)]
struct PrecomputedRunner {
    output: Arc<Mutex<Option<RunnerOutput>>>,
}

#[async_trait]
impl VirtualSnosRunner for PrecomputedRunner {
    async fn run_virtual_os(
        &self,
        _block_id: BlockId,
        _txs: Vec<InvokeTransaction>,
    ) -> Result<RunnerOutput, RunnerError> {
        Ok(self.output.lock().unwrap().take().expect("PrecomputedRunner already consumed"))
    }
}

impl<S: FlowTestState> TestRunner<S> {
    /// Runs the virtual OS, validates the output, and proves the result.
    async fn run_virtual_and_prove(self) -> VirtualSnosProverOutput {
        let test_output = self.run_virtual();
        test_output.validate();

        let runner_output = RunnerOutput {
            cairo_pie: test_output.runner_output.cairo_pie,
            l2_to_l1_messages: test_output.messages_to_l1,
        };
        let runner = PrecomputedRunner { output: Arc::new(Mutex::new(Some(runner_output))) };
        let prover = VirtualSnosProver::from_runner(runner);
        prover
            .prove_transaction(BlockId::Latest, rpc_invoke_tx(invoke_tx_args! {}))
            .await
            .expect("prove_transaction should succeed")
    }
}

/// End-to-end test for VirtualSnosProver using the flow test infrastructure.
#[ignore]
#[tokio::test]
async fn test_virtual_snos_prover_message_to_l1() {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let (mut test_builder, [contract_address]) =
        TestBuilder::<DictStateReader>::create_standard_virtual([(
            test_contract,
            calldata![Felt::ONE, Felt::TWO],
        )])
        .await;

    // Prepare message-to-L1 parameters.
    let payload = vec![Felt::from(12), Felt::from(34)];
    let message = MessageToL1 {
        from_address: contract_address,
        to_address: EthAddress::try_from(Felt::from(85)).unwrap(),
        payload: L2ToL1Payload(payload.clone()),
    };

    // Create and add the invoke transaction.
    let calldata = create_calldata(
        contract_address,
        "test_send_message_to_l1",
        &[message.to_address.into(), Felt::from(payload.len()), payload[0], payload[1]],
    );
    test_builder.add_funded_account_invoke(invoke_tx_args! { calldata });
    test_builder.messages_to_l1.push(message.clone());

    // Build, run the virtual OS, validate, and prove.
    let prover_output = test_builder.build().await.run_virtual_and_prove().await;
    let result = prover_output.result;

    // Validate messages.
    assert_eq!(result.l2_to_l1_messages, [message]);

    // Validate proof.
    verify_proof(result.proof_facts, result.proof).expect("Proof verification should succeed");
}
