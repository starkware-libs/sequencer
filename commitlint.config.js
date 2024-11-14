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
            'cairo_native',
            'ci',
            'committer',
            'common',
            'concurrency',
            'config',
            'consensus',
            'deployment',
            'execution',
            'fee',
            'helm',
            'infra',
            'JSON-RPC',
            'l1_provider',
            'load_test',
            'mempool',
            'mempool_p2p',
            'mempool_p2p_types',
            'mempool_test_utils',
            'mempool_types',
            'monitoring',
            'native_blockifier',
            'network',
            'node',
            'papyrus_state_reader',
            'protobuf',
            'release',
            'sequencer_infra',
            'sequencer_node',
            'skeleton',
            'starknet_api',
            'starknet_client',
            'starknet_gateway',
            'state',
            'storage',
            'sync',
            'test_utils',
            'tests_integration',
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
