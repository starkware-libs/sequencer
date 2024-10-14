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
            'base_layer',
            'batcher',
            'block_hash',
            'blockifier',
            'blockifier_reexecution',
            'ci',
            'committer',
            'common',
            'concurrency',
            'config',
            'consensus',
            'deployment',
            'execution',
            'fee',
            'gateway',
            'helm',
            'infra',
            'JSON-RPC',
            'l1-provider',
            'load_test',
            'mempool',
            'mempool_infra',
            'mempool_node',
            'mempool_p2p',
            'mempool_p2p_types',
            'mempool_test_utils',
            'mempool_types',
            'monitoring',
            'native_blockifier',
            'network',
            'node',
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
            'papyrus_storage',
            'papyrus_sync',
            'papyrus_test_utils',
            'protobuf',
            'release',
            'skeleton',
            'starknet_api',
            'starknet_client',
            'state',
            'storage',
            'sync',
            'test_utils',
            'tests-integration',
            'transaction',
            'types',
        ]],
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
