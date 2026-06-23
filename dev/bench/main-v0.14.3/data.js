window.BENCHMARK_DATA = {
  "lastUpdate": 1782218364004,
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
          "id": "75e8b67ede21ddee7ec9ee33f43cb01d8328bcbb",
          "message": "starknet_api: fix deploy_account V3 tx hash field ordering (nonce before DA mode) (#14428)\n\nget_deploy_account_transaction_v3_hash chained data_availability_mode\nbefore nonce, diverging from invoke_v3, declare_v3, the Cairo OS\nhash_tx_common_fields, and SNIP-8, which all chain\nchain_id -> nonce -> data_availability_mode.\n\nThe existing fixtures have nonce=0 and DA=L1=0, so the Poseidon hash was\norder-invariant and the bug was masked. Any deploy_account V3 with a\nnon-L1 nonce DA mode would hash differently in the Rust sequencer than\nin the Cairo prover/consensus.\n\nSwap the two chained fields so nonce is chained before\ndata_availability_mode.\n\nCo-authored-by: Claude Opus 4.8 (1M context) <noreply@anthropic.com>",
          "timestamp": "2026-06-10T12:57:44Z",
          "tree_id": "426097e64a6d9330992157a990f49ab380003b6c",
          "url": "https://github.com/starkware-libs/sequencer/commit/75e8b67ede21ddee7ec9ee33f43cb01d8328bcbb"
        },
        "date": 1781097109262,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 933.72875654,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1455.2247528599999,
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
          "id": "750c4d7a670568ccbbfbe4c53172f9b0e478cba3",
          "message": "starknet_proof_verifier: add negative tests for verify_proof (#14462)\n\nStarting from the valid pinned proof fixture, each test applies one\nmutation and asserts verification fails: empty proof, too-short facts,\nunsupported and V0 version markers, tampered facts, a corrupted\ncompressed byte, and corrupting/truncating the decompressed proof (so\nthe noise reaches the circuit verifier rather than failing as a mere\ndecompression error). Shared fixture loading is factored into a helper.\n\nCo-authored-by: Claude Opus 4.8 (1M context) <noreply@anthropic.com>",
          "timestamp": "2026-06-12T06:01:23Z",
          "tree_id": "4fbd2f38d0afb03c922ac05c58024fceae812034",
          "url": "https://github.com/starkware-libs/sequencer/commit/750c4d7a670568ccbbfbe4c53172f9b0e478cba3"
        },
        "date": 1781244943297,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 812.5676921200001,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1298.30734126,
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
          "id": "578fbab67b7e9c3c3e03dcf398c85dcd1ef1dca5",
          "message": "workspace: upgrade cairo compiler to v2.19.0-rc.3 (#14430)\n\n* workspace,blockifier,starknet_os_flow_tests: upgrade cairo compiler to v2.19.0-rc.3\n\nBump cairo-lang workspace dependencies and the cairo_compiler_version.txt\npin from 2.19.0-rc.2 to 2.19.0-rc.3.\n\nApply the snapshot value changes caused by rc.3 codegen:\n- blockifier secp test_secp256k1_point_from_x gas: 183190 -> 181710\n- blockifier bouncer migration_sierra_gas: 107086865 -> 106858386\n- blockifier bouncer migration_proving_gas: 231505645 -> 230976513\n- starknet_os_flow_tests experimental_libfuncs poseidon usage: 66->67, 57->58\n- starknet_os_flow_tests fuzz orchestrator address\n- central_systest_blobs cende operator + fee-token addresses, plus the\n  regenerated chain_info.json + preconfirmed_block.json and the regression\n  blob re-uploaded to GCS (generation 30 -> 39)\n\nThe fuzz cairo0/cairo1 addresses and the proof-flow fee-token and\ngenesis-root constants were verified unchanged under rc.3.\n\nrc.3 raises its MSRV to rustc 1.94. To keep the prover's stwo stack building,\nbump proving-utils (privacy-prove, privacy-circuit-verify-v1) to tag\nv0.14.3-rust-bump, which pulls the fixed stwo at git rev 489a0f3e, and bump\ncrates/starknet_transaction_prover/rust-toolchain.toml to nightly-2026-01-15\n(rustc 1.94.0-nightly). The fixed stwo was verified to compile on this\ntoolchain.\nAdapt starknet_proof_verifier and the prover test to the new privacy-circuit-verify API: the\nPrivacyProofOutput `version` field was removed (the prover now embeds the\nversion as a prefix in the proof bytes), so drop it at the construction site.\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>\n\n* starknet_proof_verifier: restore deleted proof regression test\n\nThe cairo rc.3 bump commit silently dropped the\n`regression_verify_proof_from_old_prover` test and its 0.14.3 proof\nfixtures. Restore both.\n\nThe `v0.14.3-rust-bump` proving-utils tag changed the proof format\n(\"Unknown frame descriptor\" on the old fixture), so regenerate the\n0.14.3 fixture from the PR's freshly-generated example proof, per the\ntest's documented procedure. `cargo test -p starknet_proof_verifier`\npasses (roundtrip + regression).\n\n* starknet_proof_verifier: fix negative proof tests for version-prefixed wire format\n\nThe rc.3 proving-utils (`v0.14.3-rust-bump`) changed the on-the-wire proof\nformat to a `VERSION_BYTES`-long version prefix followed by the\nzstd-compressed payload (the verifier strips it via `split_proof_version`).\nThe uncompressed-domain negative tests added in #14462 zstd-decompressed\nthe whole blob, which now fails with \"Unknown frame descriptor\" on the\nversion prefix.\n\nSplit off the version prefix before decompressing and re-prepend it after\nrecompressing, mirroring the verifier's own framing via the public\n`VERSION_BYTES`. All 10 starknet_proof_verifier tests pass.\n\n---------\n\nCo-authored-by: Claude Opus 4.8 (1M context) <noreply@anthropic.com>",
          "timestamp": "2026-06-12T09:10:25Z",
          "tree_id": "158fc2e065a0aed3131d02c96d8b32ee981f5d91",
          "url": "https://github.com/starkware-libs/sequencer/commit/578fbab67b7e9c3c3e03dcf398c85dcd1ef1dca5"
        },
        "date": 1781257137526,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 797.26243026,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1223.33698829,
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
          "id": "9cd7bf120ed9d81deb37fc863b64446bb3d9d42f",
          "message": "starknet_api: guard L1HandlerTransaction::payload_size against empty calldata (#14495)\n\npayload_size computed self.tx.calldata.0.len() - 1 unconditionally. Calldata\nhas no non-empty invariant and L1HandlerTransaction derives Deserialize, so an\nempty calldata is constructible; len() - 1 then panics in debug or wraps to\nusize::MAX in release (corrupting downstream fee/message-segment accounting).\n\nUse saturating_sub(1) so an empty calldata yields a payload size of 0. Add a\nregression test that constructs an executable L1HandlerTransaction with empty\ncalldata and asserts payload_size() == 0 (it panics on the old code).\n\nCo-authored-by: Claude Opus 4.8 (1M context) <noreply@anthropic.com>",
          "timestamp": "2026-06-15T11:19:03Z",
          "tree_id": "2d314ef6d6a5d34775a9389b71d1f4d17823d765",
          "url": "https://github.com/starkware-libs/sequencer/commit/9cd7bf120ed9d81deb37fc863b64446bb3d9d42f"
        },
        "date": 1781523288669,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 845.4945350099999,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1382.8492216,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "92669167+dafnamatsry@users.noreply.github.com",
            "name": "dafnamatsry",
            "username": "dafnamatsry"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "5114457ad4b5d6d1764b520dfa40b9e826f48854",
          "message": "starknet_api,apollo_starknet_os_program: compute OS config hash with Blake from V0_14_3 (#14499)\n\nSwitch the OS config hash from Pedersen to Blake for quantum safety, version-gated at\n  V0_14_3: blocks below it keep Pedersen + 'StarknetOsConfig3', V0_14_3 onward use\n  Blake + 'StarknetOsConfig4'. Gating keeps pre-cutover blocks re-executable/re-provable\n  against their original hash. Cairo switches straight to Blake (per-binary versioning);\n  the Rust mirror selects the hash at runtime by StarknetVersion. public_keys_hash stays\n  Pedersen.\n\n  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>",
          "timestamp": "2026-06-16T13:34:23Z",
          "tree_id": "e432d7ca9624d80ae1013974d1d591bd4d58033e",
          "url": "https://github.com/starkware-libs/sequencer/commit/5114457ad4b5d6d1764b520dfa40b9e826f48854"
        },
        "date": 1781618038599,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 954.85778625,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1327.9457589600001,
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
          "id": "773c57afc7c450a1122a57c914b10f74df2492ea",
          "message": "release: bump workspace version to 0.19.0-rc.2 (#14532)",
          "timestamp": "2026-06-17T13:29:34Z",
          "tree_id": "6edccd0932987f62c8f0b75a1ee9ae982c1b98b5",
          "url": "https://github.com/starkware-libs/sequencer/commit/773c57afc7c450a1122a57c914b10f74df2492ea"
        },
        "date": 1781704600146,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 813.64444736,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1330.32723358,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "asaf@starkware.co",
            "name": "asaf-sw",
            "username": "asaf-sw"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "539f435bd99577c884cc1441c326e7f2054a8bf8",
          "message": "papyrus_base_layer: add primary L1 endpoint down-since metric (#14576)\n\nCo-authored-by: Claude Opus 4.8 (1M context) <noreply@anthropic.com>",
          "timestamp": "2026-06-21T12:25:08Z",
          "tree_id": "530e57e1e9f9f9b16275b04d77cfff59f1dfa054",
          "url": "https://github.com/starkware-libs/sequencer/commit/539f435bd99577c884cc1441c326e7f2054a8bf8"
        },
        "date": 1782045442849,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 762.9691032100001,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1255.00402375,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "asaf@starkware.co",
            "name": "asaf-sw",
            "username": "asaf-sw"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "56b50774e42efdbef070f731eab00a3385c4ddeb",
          "message": "papyrus_base_layer: emit primary L1 endpoint down-since metric per scraper (#14577)\n\nCo-authored-by: Claude Opus 4.8 (1M context) <noreply@anthropic.com>",
          "timestamp": "2026-06-21T13:11:32Z",
          "tree_id": "abf972efd4d61d9ddae15f5eb6f6d7bd5d4285cc",
          "url": "https://github.com/starkware-libs/sequencer/commit/56b50774e42efdbef070f731eab00a3385c4ddeb"
        },
        "date": 1782048225792,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 813.90217702,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1423.22751423,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "asaf@starkware.co",
            "name": "asaf-sw",
            "username": "asaf-sw"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "76ae9783eec7a8a852d8b5ff7f452be1ca360acb",
          "message": "apollo_dashboard: alert when the primary L1 endpoint is down too long (#14578)\n\nCo-authored-by: Claude Opus 4.8 (1M context) <noreply@anthropic.com>",
          "timestamp": "2026-06-21T13:44:21Z",
          "tree_id": "80c930d9537d8ed90a1e49c3d104cbcf2abe2375",
          "url": "https://github.com/starkware-libs/sequencer/commit/76ae9783eec7a8a852d8b5ff7f452be1ca360acb"
        },
        "date": 1782050194223,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 834.9098724199999,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1312.6394082699999,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "141143145+AvivYossef-starkware@users.noreply.github.com",
            "name": "AvivYossef-starkware",
            "username": "AvivYossef-starkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "8107c2903ec28840ee0fc8df051b69a6741b1a3e",
          "message": "starknet_api: redact Proof and decode ProofFacts in Debug output (#14598)",
          "timestamp": "2026-06-23T12:22:29Z",
          "tree_id": "95b0390042f080990f1154ecbc4b0315edf3e9ec",
          "url": "https://github.com/starkware-libs/sequencer/commit/8107c2903ec28840ee0fc8df051b69a6741b1a3e"
        },
        "date": 1782218363486,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 1078.41617522,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1729.80263358,
            "unit": "ms"
          }
        ]
      }
    ]
  }
}