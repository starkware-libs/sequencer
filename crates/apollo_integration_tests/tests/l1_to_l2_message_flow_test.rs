use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::utils::{
    create_l1_to_l2_messages_args,
    end_to_end_flow,
    test_single_tx,
    EndToEndFlowArgs,
    EndToEndTestScenario,
};
use blockifier::bouncer::BouncerWeights;

// Uses end_to_end_flow with test identifier EndToEndFlowTest and instance indices [12, 13, 14, 15].
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn l1_to_l2_message_flow() {
    end_to_end_flow(
        EndToEndFlowArgs::new(
            TestIdentifier::EndToEndFlowTest,
            create_test_scenarios(),
            BouncerWeights::default().proving_gas,
        )
        .instance_indices([12, 13, 14, 15]),
    )
    .await
}

fn create_test_scenarios() -> EndToEndTestScenario {
    EndToEndTestScenario {
        create_rpc_txs_fn: |_| vec![],
        create_l1_to_l2_messages_args_fn: |tx_generator| {
            create_l1_to_l2_messages_args(tx_generator, 1, false)
        },
        test_tx_hashes_fn: test_single_tx,
    }
}
