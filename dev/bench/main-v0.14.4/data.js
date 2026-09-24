window.BENCHMARK_DATA = {
  "lastUpdate": 1790255326005,
  "repoUrl": "https://github.com/starkware-libs/sequencer",
  "entries": {
    "Benchmark": [
      {
        "commit": {
          "author": {
            "email": "asaf@starkware.co",
            "name": "Asaf Merschon",
            "username": "asaf-sw"
          },
          "committer": {
            "email": "asaf@starkware.co",
            "name": "Asaf Merschon",
            "username": "asaf-sw"
          },
          "distinct": true,
          "id": "909a814f410a687a9ed58e233900aeda92357476",
          "message": "release: bump workspace version to 0.20.0-rc.0\n\nCo-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>",
          "timestamp": "2026-08-25T07:52:07+03:00",
          "tree_id": "4251a9fd6369afd05ad6488dbca6b19810c5fa9a",
          "url": "https://github.com/starkware-libs/sequencer/commit/909a814f410a687a9ed58e233900aeda92357476"
        },
        "date": 1787634350725,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 898.19255101,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1542.61274041,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "97383386+yoavGrs@users.noreply.github.com",
            "name": "yoavGrs",
            "username": "yoavGrs"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "1f58ab3d941117992210b2ab33befdb674b824a3",
          "message": "apollo_committer,starknet_committer: add commitment infos lower bound and batched updates (#15022)",
          "timestamp": "2026-08-25T14:38:58Z",
          "tree_id": "4cf5a2f77a20beb2e1283052ba4cffe08df08c34",
          "url": "https://github.com/starkware-libs/sequencer/commit/1f58ab3d941117992210b2ab33befdb674b824a3"
        },
        "date": 1787669638199,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 885.75357097,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1447.4568641800001,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "itamar@starkware.co",
            "name": "itamar-starkware",
            "username": "itamar-starkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "87131f62de7425183f8cfa23737520227721f444",
          "message": "starknet_committer,apollo_storage,apollo_batcher: add version field to compressed commitment infos (#15031)",
          "timestamp": "2026-08-26T08:21:50Z",
          "tree_id": "5a2586d733988d1f39a134c402e827339696a89f",
          "url": "https://github.com/starkware-libs/sequencer/commit/87131f62de7425183f8cfa23737520227721f444"
        },
        "date": 1787733738670,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 890.93660213,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1414.84014438,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "97383386+yoavGrs@users.noreply.github.com",
            "name": "yoavGrs",
            "username": "yoavGrs"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "4b46b703cd47ddc41e40d3d803dde041c022d240",
          "message": "apollo_committer,starknet_committer: compress commitment infos once and store the compressed payload (#15032)",
          "timestamp": "2026-08-26T10:15:59Z",
          "tree_id": "1630d10ea2e128adbade2de9219a9ee0b2ff2fe6",
          "url": "https://github.com/starkware-libs/sequencer/commit/4b46b703cd47ddc41e40d3d803dde041c022d240"
        },
        "date": 1787740691065,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 933.06071405,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1412.61764182,
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
          "id": "4e8046138c4f3ef721ede1dd87cb12f8ee078c61",
          "message": "apollo_versioned_constants,apollo_consensus_orchestrator: version the L2 gas price multiplier (#15051)\n\nMAX_GAS_PRICE_MULTIPLIER was a private constant in fee_market, so the ceiling it\nsets could not move with the Starknet version. It becomes\nVersionedConstants::max_gas_price_multiplier, stated explicitly by every\nversion's resource as 10, which is the value it already had.\n\nPricing is unchanged: the 0.14.0 through 0.14.4 resources all carry the same 10\nthey compiled in before. The compile-time assert that the multiplier is above\none becomes max_gas_price_multiplier_is_above_one_in_every_version, a test that\nchecks the invariant for every version.\n\nMerging forward to main: main carries\norchestrator_versioned_constants_0_14_5.json, which this branch cannot touch,\nsince that file does not exist on the main-v0.14.4 lineage. The merge must add\n\"max_gas_price_multiplier\": 10 to it. Nothing conflicts textually, but the step\nis mandatory: without the key, serde_json::from_str on that resource fails and\nthe first VersionedConstants::latest_constants() call panics, since V0_14_5 is\nLATEST on main. Two tests on main catch the omission:\nmax_gas_price_multiplier_is_above_one_in_every_version, which deserializes every\nversion's resource, and test_vc_diffs_regression, which diffs the raw JSON of\nconsecutive versions, so a 0_14_5 without the key turns the empty\n0.14.4_0.14.5.txt into \"- /max_gas_price_multiplier\".\n\nCo-authored-by: Claude Opus 5 (1M context) <noreply@anthropic.com>",
          "timestamp": "2026-08-30T06:07:07Z",
          "tree_id": "b5fcd20dc2e58b0c14349870a14f7f3980a98820",
          "url": "https://github.com/starkware-libs/sequencer/commit/4e8046138c4f3ef721ede1dd87cb12f8ee078c61"
        },
        "date": 1788071283151,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 835.35711259,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1281.36917251,
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
          "id": "7d512231a47c2074e54e7320fa99ff9c3480a829",
          "message": "starknet_api: add proof facts version V2 (#15013)\n\nAdds the PROOF_VERSION_V2 marker ('PROOF2', 0x50524f4f4632) and\nProofVersion::V2, so proof facts carrying it parse and round-trip.\n\nPurely additive: no protocol version allows V2 yet (allowed_proof_versions\nis unchanged), the OS still asserts V1, and the proof verifier rejects V2\nup front to keep its match exhaustive. Both placeholder spots carry a\nTODO(Einat) pointing at what to change once the OS and the V2 circuit are\nwired up, which the rest of the stack does.\n\nCo-authored-by: Claude Opus 5 (1M context) <noreply@anthropic.com>",
          "timestamp": "2026-08-30T08:31:18Z",
          "tree_id": "52de1faecb0795554576e08cb078b9412302b557",
          "url": "https://github.com/starkware-libs/sequencer/commit/7d512231a47c2074e54e7320fa99ff9c3480a829"
        },
        "date": 1788080398298,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 811.4074406,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1316.6301643499999,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "97383386+yoavGrs@users.noreply.github.com",
            "name": "yoavGrs",
            "username": "yoavGrs"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "01360393c6b431cdc1d57480aee6c983afa59248",
          "message": "apollo_committer,starknet_committer: forward stored compressed commitment infos on historical replay (#15070)",
          "timestamp": "2026-08-30T09:24:18Z",
          "tree_id": "ac853e59b67db308fe8e8a5d375ccc549302551e",
          "url": "https://github.com/starkware-libs/sequencer/commit/01360393c6b431cdc1d57480aee6c983afa59248"
        },
        "date": 1788082741005,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 901.778741,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1277.15059816,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "97383386+yoavGrs@users.noreply.github.com",
            "name": "yoavGrs",
            "username": "yoavGrs"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "532f51d1fd18a88c34aac1b58f1b2b0421ec9f15",
          "message": "apollo_committer,starknet_committer: avoid cloning compressed commitment infos on every block commit (#15077)",
          "timestamp": "2026-08-31T08:42:52Z",
          "tree_id": "52fd36b6fecc546f7f674af219eb7644e05da9a6",
          "url": "https://github.com/starkware-libs/sequencer/commit/532f51d1fd18a88c34aac1b58f1b2b0421ec9f15"
        },
        "date": 1788168336890,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 940.07315016,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1414.38605476,
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
          "id": "c52d575ea25784405e81b8e9320cd239b1d7cb15",
          "message": "workspace,apollo_starknet_os_program,blockifier,starknet_proof_verifier: switch to proof version V2 (#15014)\n\nReplaces proof version V1 with V2 for client-side proving.\n\n- Cairo OS: PROOF_VERSION_V1 -> PROOF_VERSION_V2 ('PROOF2'), asserted by\n  check_proof_facts. Only the `os` program hash changes.\n- starknet_os: Const::ProofVersionV1 -> Const::ProofVersionV2.\n- blockifier: allowed_proof_versions for 0.14.4 becomes [V2].\n- starknet_proof_verifier: pins privacy-circuit-verify and privacy-prove to\n  starkware-libs/proving, dispatches V2 to the V2 circuit, and rejects V0\n  and V1. try_into_proof_facts stamps V2.\n- Regenerates the program hash, proof fixtures and derived test fixtures.\n\nCo-authored-by: Claude Opus 5 (1M context) <noreply@anthropic.com>",
          "timestamp": "2026-09-01T09:36:47Z",
          "tree_id": "b590977f42c7de2b6c47eb07cb9edd504499a114",
          "url": "https://github.com/starkware-libs/sequencer/commit/c52d575ea25784405e81b8e9320cd239b1d7cb15"
        },
        "date": 1788257132537,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 923.1492602100001,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1509.3789061500001,
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
          "id": "d202a9c1c4f90233ac8bfbc4d6b827a7694950bb",
          "message": "starknet_api,apollo_rpc_execution,blockifier_reexecution: move execution consts to starknet_api (#14783)\n\nblockifier_reexecution's only use of apollo_rpc_execution was three constants,\ndragging apollo_storage/apollo_infra/libmdbx into the starknet_transaction_prover\nbinary's dependency graph through an otherwise-unnecessary edge. Move\nDEPRECATED_CONTRACT_SIERRA_SIZE and the fee token address statics to starknet_api\nand have apollo_rpc_execution re-export them, so blockifier_reexecution can drop\nthe dependency entirely.\n\nSevering that edge also removes starknet_transaction_prover's only path to\napollo_infra, the sole crate enabling tracing-subscriber's \"json\" feature. The\nprover's main.rs calls fmt::layer().json() but declared only \"env-filter\", so it\nhad been compiling on a feature it never asked for; it now requests \"json\"\nitself.",
          "timestamp": "2026-09-14T19:09:22Z",
          "tree_id": "74c46107efbab3fc6dae2e17000f7f8ad7c3e89c",
          "url": "https://github.com/starkware-libs/sequencer/commit/d202a9c1c4f90233ac8bfbc4d6b827a7694950bb"
        },
        "date": 1789415805638,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 977.24881,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1522.6845512300001,
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
          "id": "f971cfda4799ddfd3284c618562020951034512f",
          "message": "workspace,apollo_starknet_os_program,blockifier,starknet_proof_verifier: accept V1 and V2 proofs (#15120)\n\nMakes the verifier, the OS and the blockifier accept proof version V1 alongside\nV2, so proofs from the previously deployed prover stay valid while V2 rolls out.\nThe in-repo transaction prover is unchanged and keeps producing V2 only.\n\n- Workspace: restores privacy-circuit-verify from proving-utils as the -v1\n  dependency, next to -v2 from proving. Purely additive: no shared dependency\n  changes version.\n- starknet_proof_verifier: verify_proof dispatches V1 to the V1 circuit and V2\n  to the V2 circuit; V0 stays rejected. try_into_proof_facts still stamps V2,\n  the only version built here.\n- Cairo OS: check_proof_facts accepts PROOF_VERSION_V1 or PROOF_VERSION_V2.\n  Only the `os` program hash changes; regenerated.\n- starknet_os: restores Const::ProofVersionV1.\n- blockifier: allowed_proof_versions for 0.14.4 becomes [V1, V2].\n\nCo-authored-by: Claude Opus 5 (1M context) <noreply@anthropic.com>",
          "timestamp": "2026-09-15T06:30:09Z",
          "tree_id": "21ea940d5aff2b076deafe84987c2aa5669fcb96",
          "url": "https://github.com/starkware-libs/sequencer/commit/f971cfda4799ddfd3284c618562020951034512f"
        },
        "date": 1789455470384,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 971.8617271,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1383.27107783,
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
          "distinct": false,
          "id": "95286058d0fb08e445d231bcb6cc4a89743ed5be",
          "message": "apollo_rpc_types,apollo_rpc,apollo_gateway_types: extract rpc error types into apollo_rpc_types (#14784)\n\napollo_gateway_types only needed apollo_rpc for its JSON-RPC error constants,\nwhich dragged apollo_storage, blockifier, and jsonrpsee-server into\napollo_http_server and apollo_mempool_p2p through a component that has no\nother use for them. Extract the error module verbatim into a new\napollo_rpc_types crate depending only on jsonrpsee-types, and have\napollo_gateway_types depend on that instead. apollo_rpc re-exports the module\nso its own internal paths stay unchanged.",
          "timestamp": "2026-09-15T15:45:23Z",
          "tree_id": "9898e566851c1c82b68ab13a9d52ba3b467eaf9e",
          "url": "https://github.com/starkware-libs/sequencer/commit/95286058d0fb08e445d231bcb6cc4a89743ed5be"
        },
        "date": 1789488583533,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 913.32494288,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1360.3439101600002,
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
          "id": "7a3a2d4bd7e902ea4ab871a674a0ac0de99d3b38",
          "message": "apollo_config,apollo_starknet_client,apollo_central_sync_config: move RetryConfig to apollo_config (#14785)\n\napollo_central_sync_config's only use of apollo_starknet_client was RetryConfig,\ndragging the reqwest(blocking)/tokio-full/cairo-class-parsing chain into the\nconfig crate stack through an edge that only needed a plain value type. Move\nRetryConfig into apollo_config next to the other small reusable config value\ntypes, and have apollo_starknet_client re-export it so its own executor and\ncall sites stay unchanged.",
          "timestamp": "2026-09-16T07:51:56Z",
          "tree_id": "2e8c5ff44f40029f40ec1bdc428b5ccfc8a855c7",
          "url": "https://github.com/starkware-libs/sequencer/commit/7a3a2d4bd7e902ea4ab871a674a0ac0de99d3b38"
        },
        "date": 1789547170638,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 965.98617855,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1482.39089012,
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
          "id": "e6bd04ea721d6a9676a1e7ab93a28b4b87e47f44",
          "message": "apollo_compile_to_casm,apollo_sierra_compilation_config: add a bundled libfunc list option (#15133)\n\nReplace `audited_libfuncs_only: bool` with `allowed_libfuncs_list`: \"audited\", \"all\", or \"bundled\"\n— the list in `apollo_compile_to_casm/resources/allowed_libfuncs.json`, shipped in the runtime\nimage next to the config schema and resolved at startup.\n\nCo-authored-by: Claude Opus 5 (1M context) <noreply@anthropic.com>",
          "timestamp": "2026-09-16T11:45:43Z",
          "tree_id": "9935f16cd58b806af654cd12beac99f09cca8210",
          "url": "https://github.com/starkware-libs/sequencer/commit/e6bd04ea721d6a9676a1e7ab93a28b4b87e47f44"
        },
        "date": 1789560190710,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 957.78010205,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1527.40842525,
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
          "id": "9cf4dd8589fe2eaac6326bb1b08c4f4d24b98837",
          "message": "workspace: upgrade cairo compiler to v2.19.5 (#15143)\n\nBumps the Rust cairo-lang-* crates and the Sierra compiler binary\nversion from 2.19.4 to 2.19.5. The bump moves the Sierra version\nfrom 1.9.3 to 1.9.4, cascading into Cairo1 class hashes and their\ndownstream fixtures: starknet_os_flow_tests fuzz-deployment addresses\nand hint-coverage fixtures, the proof-flow genesis global root and\nSTRK fee-token address, and the cende blob regression (operator and\nfee-token addresses, chain_info/preconfirmed_block).\n\nThe 2.19.5 class-hash changes alter the cende blobs, so the regression\nnow compares against a freshly uploaded generation 62 in the\napollo-central-systest-blobs bucket.\n\nThe hint-coverage fixtures gain StatelessHint(EnterScopeNewNode): the\nchanged class hashes reshape the Patricia tree, so the traversal now\nenters a new node where it previously did not. The experimental-libfuncs\nposeidon counts drop (66 -> 64, 57 -> 55) from compiler output changes\nin 2.19.5.\n\nCo-authored-by: Claude Opus 5 (1M context) <noreply@anthropic.com>\nCo-authored-by: Cursor <cursoragent@cursor.com>",
          "timestamp": "2026-09-22T08:26:51Z",
          "tree_id": "957c500b5c4628fa398a3b79df9f1a2fc44f7b27",
          "url": "https://github.com/starkware-libs/sequencer/commit/9cf4dd8589fe2eaac6326bb1b08c4f4d24b98837"
        },
        "date": 1790066821860,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 923.68959099,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1473.0961843,
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
          "id": "1860b9850b5c51858c50c0f5038933e49f932eef",
          "message": "workspace: bump workspace version to 0.20.0-rc.1 (#15147)",
          "timestamp": "2026-09-23T13:03:44Z",
          "tree_id": "a61a842d71a3cacdeee2c125bcdce0dd6c319592",
          "url": "https://github.com/starkware-libs/sequencer/commit/1860b9850b5c51858c50c0f5038933e49f932eef"
        },
        "date": 1790169816487,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 953.38821462,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1368.69877369,
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
          "id": "ceb15c1ccc9ac2089a382f290ce086b78d4707e7",
          "message": "apollo_batcher_config,apollo_config,starknet_api: add a storage access filter config (#15152)\n\nCo-authored-by: Claude Opus 5.5 (1M context) <noreply@anthropic.com>",
          "timestamp": "2026-09-24T12:45:59Z",
          "tree_id": "f45a89ab60cbc30043b726ca1e357fc4b04965f5",
          "url": "https://github.com/starkware-libs/sequencer/commit/ceb15c1ccc9ac2089a382f290ce086b78d4707e7"
        },
        "date": 1790255325309,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 903.44534629,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1377.64225877,
            "unit": "ms"
          }
        ]
      }
    ]
  }
}