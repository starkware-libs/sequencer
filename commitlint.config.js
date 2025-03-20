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
            "apollo_reverts",
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
            'papyrus_config',
            'papyrus_execution',
            'papyrus_load_test',
            'papyrus_monitoring_gateway',
            'papyrus_network',
            'papyrus_network_types',
            'papyrus_node',
            'papyrus_p2p_sync',
            'papyrus_proc_macros',
            'papyrus_protobuf',
            'papyrus_rpc',
            'papyrus_state_reader',
            'papyrus_storage',
            'papyrus_sync',
            'papyrus_test_utils',
            'release',
            'shared_execution_objects',
            'starknet_api',
            'starknet_batcher',
            'starknet_batcher_types',
            'starknet_client',
            'starknet_committer',
            'starknet_committer_and_os_cli',
            'starknet_consensus_manager',
            'starknet_consensus_orchestrator',
            'starknet_class_manager',
            'starknet_class_manager_types',
            'starknet_gateway',
            'starknet_gateway_types',
            'starknet_http_server',
            'starknet_infra_utils',
            'starknet_integration_tests',
            'starknet_l1_gas_price', 
            'starknet_l1_gas_price_types',
            'starknet_l1_provider',
            'starknet_l1_provider_types',
            'starknet_mempool',
            'starknet_mempool_p2p',
            'starknet_mempool_p2p_types',
            'starknet_mempool_types',
            'starknet_monitoring_endpoint',
            'starknet_os',
            'starknet_patricia',
            'starknet_patricia_storage',
            'starknet_sequencer_deployments',
            'starknet_sequencer_infra',
            'starknet_sequencer_dashboard',
            'starknet_sequencer_metrics',
            'starknet_sequencer_node',
            'starknet_sierra_multicompile',
            'starknet_sierra_multicompile_types',
            'starknet_state_sync',
            'starknet_state_sync_types',
            'starknet_task_executor',
            'workspace_tests',
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
