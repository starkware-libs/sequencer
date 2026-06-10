window.BENCHMARK_DATA = {
  "lastUpdate": 1781096170811,
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
          "id": "8596488a0c93b6b746bbfcda94bde2c833887759",
          "message": "workspace: bump privacy-circuit-verify-v1 and privacy-prove to v0.14.3-rc-1 (#14254)\n\nFollow-up to #14241 (which moved both to v0.14.3-rc-0). v0.14.3-rc-1 is the\nproving-utils tag of commit c0b937b (PR starkware-libs/proving-utils#355,\n\"Bump stwo-circuits.\"), which advances the stwo-circuits revision from\n9ff7fc8 to 618db0a. privacy-circuit-verify-v0 stays on 580135e.\n\nstwo-circuits changes the on-chain proof bytes, so regenerate the three\nserialized proof fixtures:\n  - proof_flow/proof.bin (apollo_integration_tests) via\n    `cargo test -p starknet_os_flow_tests --features\n    starknet_transaction_prover/stwo_proving --release generate_proof_fixtures\n    -- --ignored`\n  - example_proof.bin (apollo_transaction_converter) via\n    `cargo test -p starknet_transaction_prover --features stwo_proving\n    --release -- --ignored regenerate_proof_fixtures`\n  - regression_test/0.14.3/example_proof.bin (starknet_proof_verifier) is a\n    copy of the apollo_transaction_converter fixture.\n\nCo-authored-by: Claude Opus 4.7 (1M context) <noreply@anthropic.com>",
          "timestamp": "2026-05-28T17:02:36Z",
          "tree_id": "1f8e9f7cdf0c0c0ba5924c4d4e8af15d56882b1f",
          "url": "https://github.com/starkware-libs/sequencer/commit/8596488a0c93b6b746bbfcda94bde2c833887759"
        },
        "date": 1779988996749,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 905.0005125,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1349.23592186,
            "unit": "ms"
          }
        ]
      },
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
          "id": "b13c7394dba713966064a71272286fdd5c04ec9c",
          "message": "Merge pull request #14315 from starkware-libs/dori/merge-main-v0.14.2-into-main-v0.14.3-1780483904\n\nMerge main-v0.14.2 into main-v0.14.3",
          "timestamp": "2026-06-03T14:24:27Z",
          "tree_id": "bf1f26f29b7216c434a6f0eede714ccb98853b2d",
          "url": "https://github.com/starkware-libs/sequencer/commit/b13c7394dba713966064a71272286fdd5c04ec9c"
        },
        "date": 1780497532282,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 913.43093491,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1280.69377444,
            "unit": "ms"
          }
        ]
      },
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
          "id": "8fec81e6f7333485a86281764af7144bf1c25998",
          "message": "release: bump compiler version to 2.19.0-rc.2 (#14314)\n\nSigned-off-by: Dori Medini <dori@starkware.co>",
          "timestamp": "2026-06-03T15:29:15Z",
          "tree_id": "bb70d47b3f34b2f8223e401edeec33ebc2136315",
          "url": "https://github.com/starkware-libs/sequencer/commit/8fec81e6f7333485a86281764af7144bf1c25998"
        },
        "date": 1780501521534,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 916.58008662,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1374.75226284,
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
          "id": "66b2ac270c2caea504f9b83fbda3f804458ab0d7",
          "message": "workspace: bump privacy-circuit-verify-v1 and privacy-prove to v0.14.3 (#14409)\n\nFollow-up to #14254 (which moved both to v0.14.3-rc-1). v0.14.3 is the\nproving-utils tag of commit abdc99c (same commit as v0.14.3-rc-2). Upstream\nchanges since v0.14.3-rc-1:\n- PrivacyProofOutput gained a version field recording which privacy-prove\n  version generated the proof (proving-utils#349): populate it at the two\n  sequencer construction sites; the verifier ignores it.\n- stwo-circuits bumped 618db0a -> 24f39918 (proving-utils#361), changing the\n  circuit preprocessed root and therefore the on-chain proof bytes.\n\nSince the proof bytes change, regenerate the serialized proof fixtures:\n- proof_flow/proof.bin (apollo_integration_tests) via\n  `cargo +nightly-2025-07-14 test -p starknet_os_flow_tests --features\n  starknet_transaction_prover/stwo_proving --release generate_proof_fixtures\n  -- --ignored`\n- example_proof.bin (apollo_transaction_converter) via\n  `cargo +nightly-2025-07-14 test -p starknet_transaction_prover --features\n  stwo_proving --release -- --ignored regenerate_proof_fixtures`\n- regression_test/0.14.3/example_proof.bin (starknet_proof_verifier) is a\n  copy of the apollo_transaction_converter fixture.\nThe proof facts JSONs are unchanged: the circuit bump changes proof bytes but\nnot the program output.\n\nCo-authored-by: Claude Opus 4.8 (1M context) <noreply@anthropic.com>",
          "timestamp": "2026-06-07T15:38:56Z",
          "tree_id": "73fc72478d291b682c3d7befe477c164bca8a7b0",
          "url": "https://github.com/starkware-libs/sequencer/commit/66b2ac270c2caea504f9b83fbda3f804458ab0d7"
        },
        "date": 1780848208218,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 1392.74944165,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1712.9419976,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "avi.cohen@starkware.co",
            "name": "Avi Cohen",
            "username": "avi-starkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7b6e4162c07b5a48544f8b71524225f9f1a59eb6",
          "message": "apollo_http_server,blockifier_reexecution,starknet_api: move tx json deserializer to starknet_api (#14408)\n\nCo-authored-by: Claude Opus 4.8 (1M context) <noreply@anthropic.com>",
          "timestamp": "2026-06-08T10:31:32Z",
          "tree_id": "4e12069e2cd4ab7868dc81f8613ae3ec5311d74e",
          "url": "https://github.com/starkware-libs/sequencer/commit/7b6e4162c07b5a48544f8b71524225f9f1a59eb6"
        },
        "date": 1780915568379,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 858.66186678,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1282.7753702999998,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "avi.cohen@starkware.co",
            "name": "Avi Cohen",
            "username": "avi-starkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7db57cab032e272e189296b58866535e2557e458",
          "message": "blockifier_reexecution: compile Sierra to Casm in-process via library call (#14406)\n\nCo-authored-by: Claude Opus 4.8 (1M context) <noreply@anthropic.com>",
          "timestamp": "2026-06-08T11:20:00Z",
          "tree_id": "57ed905d92db8387f7478b1ee8084be172fb78e0",
          "url": "https://github.com/starkware-libs/sequencer/commit/7db57cab032e272e189296b58866535e2557e458"
        },
        "date": 1780918412094,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 923.3813019099999,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1426.37206782,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "einat@starkware.co",
            "name": "einat-starkware",
            "username": "einat-starkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "31e7cdf34ca5c824bc86e05915bd3ed948facfe8",
          "message": "workspace: bump version to 0.19.0-rc.1 (#14414)",
          "timestamp": "2026-06-08T11:27:00Z",
          "tree_id": "e0d51f1b6495985f0a2d16005d06a822179b4c95",
          "url": "https://github.com/starkware-libs/sequencer/commit/31e7cdf34ca5c824bc86e05915bd3ed948facfe8"
        },
        "date": 1780919181569,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 885.16437033,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1371.06116097,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "einat@starkware.co",
            "name": "einat-starkware",
            "username": "einat-starkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "aaf7c0b674466600a176d6aad5ac8f33c0e3b694",
          "message": "starknet_os,starknet_api,starknet_proof_verifier: remove support for proof version 0 (#14432)",
          "timestamp": "2026-06-10T12:34:09Z",
          "tree_id": "866323f50abb6d2b278aea438a2d5c2df94b905c",
          "url": "https://github.com/starkware-libs/sequencer/commit/aaf7c0b674466600a176d6aad5ac8f33c0e3b694"
        },
        "date": 1781096170391,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 958.72187779,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1380.19534827,
            "unit": "ms"
          }
        ]
      }
    ]
  }
}