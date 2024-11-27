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
        'scope-enum': [2, 'always', [
            'blockifier',
            'blockifier_reexecution',
            'cairo_native',
            'ci',
            'committer',
            'committer_cli',
            'deployment',
            'helm',
            'infra_utils',
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
            'sequencing',
            'starknet_api',
            'starknet_batcher',
            'starknet_batcher_types',
            'starknet_client',
            'starknet_committer',
            'starknet_consensus_manager',
            'starknet_gateway',
            'starknet_gateway_types',
            'starknet_http_server',
            'starknet_integration_tests',
            'starknet_l1_provider',
            'starknet_mempool',
            'starknet_mempool_p2p',
            'starknet_mempool_p2p_types',
            'starknet_mempool_types',
            'starknet_monitoring_endpoint',
            'starknet_patricia',
            'starknet_sequencer_infra',
            'starknet_sequencer_node',
            'starknet_sierra_compile',
            'starknet_state_sync',
            'starknet_state_sync_types',
            'starknet_task_executor',
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
