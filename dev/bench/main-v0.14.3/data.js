window.BENCHMARK_DATA = {
  "lastUpdate": 1779983915957,
  "repoUrl": "https://github.com/starkware-libs/sequencer",
  "entries": {
    "Benchmark": [
      {
        "commit": {
          "author": {
            "email": "dori@starkware.co",
            "name": "dorimedini-starkware",
            "username": "dorimedini-starkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9c562b3d76cd87e1c9ac1b4ea84f5a5399fbb1f3",
          "message": "release: set workspace crate versions to 0.19.0-rc.0 (#14235)\n\nSigned-off-by: Dori Medini <dori@starkware.co>",
          "timestamp": "2026-05-28T08:45:32Z",
          "tree_id": "eb670a561fcdae03d82d2edad48957a07c31b82b",
          "url": "https://github.com/starkware-libs/sequencer/commit/9c562b3d76cd87e1c9ac1b4ea84f5a5399fbb1f3"
        },
        "date": 1779958656612,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 859.91449225,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1346.1030244,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "78365039+Yoni-Starkware@users.noreply.github.com",
            "name": "Yoni",
            "username": "Yoni-Starkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "8e1aed58e209f1973b735cfb26ad60cdcd0c16e6",
          "message": "workspace: bump privacy-circuit-verify-v1 and privacy-prove to v0.14.3-rc-0 (#14241)\n\nPoint the v1 variant of privacy-circuit-verify and privacy-prove at the\nproving-utils v0.14.3-rc-0 release tag (commit ea3b062). Pulls in matching\nstwo-circuits (9ff7fc8); stwo, stwo-cairo and cairo-air match main-v0.14.3.\nprivacy-circuit-verify-v0 stays on 580135e.\n\nThe new privacy-circuit-verify-v1 changes the serialized proof format, so\nproof fixtures must be regenerated:\n  - proof_flow/proof.bin (apollo_integration_tests) via\n    `cargo test -p starknet_os_flow_tests --features\n    starknet_transaction_prover/stwo_proving --release generate_proof_fixtures\n    -- --ignored`\n  - example_proof.bin (apollo_transaction_converter) via\n    `cargo test -p starknet_transaction_prover --features stwo_proving\n    --release -- --ignored regenerate_proof_fixtures`\n  - regression_test/0.14.3/example_proof.bin (starknet_proof_verifier) is a\n    copy of the apollo_transaction_converter fixture, updated to track the\n    new on-chain 0.14.3 proof format.\n\nCo-authored-by: Claude Opus 4.7 (1M context) <noreply@anthropic.com>",
          "timestamp": "2026-05-28T15:42:38Z",
          "tree_id": "a0df3baae1ae2f33016bcfa85a552548f7e286fb",
          "url": "https://github.com/starkware-libs/sequencer/commit/8e1aed58e209f1973b735cfb26ad60cdcd0c16e6"
        },
        "date": 1779983915394,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 926.63184188,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1425.31555989,
            "unit": "ms"
          }
        ]
      }
    ]
  }
}