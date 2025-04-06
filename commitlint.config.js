const Configuration = {
    /*
     * Resolve and load @commitlint/config-conventional from node_modules.
     * Referenced packages must be installed
     */
    extends: ['@commitlint/config-conventional'],
    /*
     * Resolve and load conventional-changelog-atom from node_modules.
     * Referenced packages must be installed
     */
    // parserPreset: 'conventional-changelog-atom',
    /*
     * Resolve and load @commitlint/format from node_modules.
     * Referenced package must be installed
     */
    formatter: '@commitlint/format',
    /*
     * Any rules defined here will override rules from @commitlint/config-conventional
     */
    rules: {
        'scope-empty': [2, 'never'],
        'scope-enum': [2, 'always', [
            'apollo_batcher',
            'apollo_batcher_types',
            'apollo_central_sync',
            'apollo_class_manager',
            'apollo_class_manager_types',
            'apollo_dashboard',
            'apollo_deployments',
            'apollo_config',
            'apollo_consensus_manager',
            'apollo_consensus_orchestrator',
            'apollo_gateway',
            'apollo_gateway_types',
            'apollo_http_server',
            'apollo_infra',
            'apollo_infra_utils',
            'apollo_integration_tests',
            'apollo_l1_gas_price',
            'apollo_l1_gas_price_types',
            'apollo_l1_provider',
            'apollo_l1_provider_types',
            'apollo_mempool',
            'apollo_mempool_p2p',
            'apollo_mempool_p2p_types',
            'apollo_mempool_types',
            'apollo_metrics',
            'apollo_monitoring_endpoint',
            'apollo_network',
            'apollo_network_types',
            'apollo_node',
            'apollo_p2p_sync',
            'apollo_proc_macros',
            'apollo_protobuf',
            'apollo_reverts',
            'apollo_rpc',
            'apollo_rpc_execution',
            'apollo_sierra_multicompile',
            'apollo_sierra_multicompile_types',
            'apollo_starknet_client',
            'apollo_state_reader',
            'apollo_state_sync',
            'apollo_state_sync_types',
            'apollo_storage',
            'apollo_task_executor',
            'apollo_test_utils',
            'blockifier',
            'blockifier_reexecution',
            'blockifier_test_utils',
            'cairo_native',
            'ci',
            'committer',
            'consensus',
            'deployment',
            'infra',
            'mempool_test_utils',
            'native_blockifier',
            'papyrus_base_layer',
            'papyrus_common',
            'papyrus_load_test',
            'papyrus_monitoring_gateway',
            'papyrus_node',
            'release',
            'shared_execution_objects',
            'starknet_api',
            'starknet_committer',
<<<<<<< HEAD
            'starknet_committer_and_os_cli',
            'starknet_os',
||||||| 05c74b1e9
            'starknet_consensus_manager',
            'starknet_class_manager_types',
            'starknet_gateway',
            'starknet_gateway_types',
            'starknet_http_server',
            'starknet_infra_utils',
            'starknet_integration_tests',
            'starknet_l1_provider',
            'starknet_l1_provider_types',
            'starknet_mempool',
            'starknet_mempool_p2p',
            'starknet_mempool_p2p_types',
            'starknet_mempool_types',
            'starknet_monitoring_endpoint',
=======
            'starknet_compilation_utils',
            'starknet_compile_to_casm',
            'starknet_compile_to_native',
            'starknet_consensus_manager',
            'starknet_class_manager_types',
            'starknet_gateway',
            'starknet_gateway_types',
            'starknet_http_server',
            'starknet_infra_utils',
            'starknet_integration_tests',
            'starknet_l1_provider',
            'starknet_l1_provider_types',
            'starknet_mempool',
            'starknet_mempool_p2p',
            'starknet_mempool_p2p_types',
            'starknet_mempool_types',
            'starknet_monitoring_endpoint',
>>>>>>> origin/main-v0.13.5
            'starknet_patricia',
<<<<<<< HEAD
            'starknet_patricia_storage',
            'workspace_tests',
||||||| 05c74b1e9
            'starknet_sequencer_infra',
            'starknet_sequencer_node',
            'starknet_sierra_multicompile',
            'starknet_state_sync',
            'starknet_state_sync_types',
            'starknet_task_executor',
=======
            'starknet_sequencer_infra',
            'starknet_sequencer_node',
            'starknet_state_sync',
            'starknet_state_sync_types',
            'starknet_task_executor',
>>>>>>> origin/main-v0.13.5
        ]],
        'header-max-length': [2, 'always', 100],
    },
    /*
     * Functions that return true if commitlint should ignore the given message.
     */
    ignores: [(commit) => commit === ''],
    /*
     * Whether commitlint uses the default ignore rules.
     */
    defaultIgnores: true,
    /*
     * Custom URL to show upon failure
     */
    helpUrl:
        'https://github.com/conventional-changelog/commitlint/#what-is-commitlint',
    /*
     * Custom prompt configs, not used currently.
     */
    prompt: {
        messages: {},
        questions: {
            type: {
                description: 'please input type:',
            },
        },
    },
};

module.exports = Configuration;
