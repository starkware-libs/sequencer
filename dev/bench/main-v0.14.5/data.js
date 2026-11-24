window.BENCHMARK_DATA = {
  "lastUpdate": 1795522124727,
  "repoUrl": "https://github.com/starkware-libs/sequencer",
  "entries": {
    "Benchmark": [
      {
        "commit": {
          "author": {
            "email": "einat@starkware.co",
            "name": "Einat Gavish",
            "username": "einat-starkware"
          },
          "committer": {
            "email": "einat@starkware.co",
            "name": "Einat Gavish",
            "username": "einat-starkware"
          },
          "distinct": true,
          "id": "cd1f5646a124239369e380b892fcb2d0f3c2e101",
          "message": "release: open main-v0.14.5 release branch from main-v0.14.4\n\nBump crate versions to 0.21.0-rc.0, register main-v0.14.5 in merge_paths.json\nand parent_branch.txt, and add Starknet version 0.14.5 with versioned\nconstants copied unchanged from 0.14.4.\n\nCo-Authored-By: Claude Fable 5.2 <noreply@anthropic.com>",
          "timestamp": "2026-11-23T10:55:31+02:00",
          "tree_id": "edd8d2b5a5666e1b04806082ef11addfa28e2da2",
          "url": "https://github.com/starkware-libs/sequencer/commit/cd1f5646a124239369e380b892fcb2d0f3c2e101"
        },
        "date": 1795425308747,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 897.77484367,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1290.17631932,
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
          "id": "c68ee7a7d73b11e2b395f0868211e63028884533",
          "message": "release: regenerate proof-flow fixtures and drop the cende blob test for v0.14.5 (#15307)\n\nMoving the latest Starknet version to 0.14.5 changes the genesis block hash\nbaked into the proof-flow fixtures, so regenerate them. Drop the cende blob\nregression test (superseded by the CenDe system test; 0.14.4 is its cutoff\nversion, as on main).\n\nCo-authored-by: Claude Fable 5.2 <noreply@anthropic.com>",
          "timestamp": "2026-11-23T12:59:32+02:00",
          "tree_id": "aeb68971664ab349b52ce85f8794df58e04f471a",
          "url": "https://github.com/starkware-libs/sequencer/commit/c68ee7a7d73b11e2b395f0868211e63028884533"
        },
        "date": 1795433534620,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 1150.06082748,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1345.3369331,
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
          "id": "12162f2edf12cb6a0b89444a6b0ef217c9347a9b",
          "message": "workspace: bump workspace version to 0.21.0-rc.1 (#15316)\n\nCo-authored-by: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-11-24T07:05:12Z",
          "tree_id": "ca2bb96c49e79fd049baa9a2fe0ac228cde8654a",
          "url": "https://github.com/starkware-libs/sequencer/commit/12162f2edf12cb6a0b89444a6b0ef217c9347a9b"
        },
        "date": 1795505304689,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 1041.33038441,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1491.3329264000001,
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
          "distinct": false,
          "id": "1cd5da0a10f0b571b9a6f8c9d18c8295c39698f3",
          "message": "starknet_os_flow_tests: test the virtual OS class blocklist (#15309)\n\nCo-authored-by: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-11-24T07:41:28Z",
          "tree_id": "069b74c9604e14175e59e8f40d7b1c33fb9b048f",
          "url": "https://github.com/starkware-libs/sequencer/commit/1cd5da0a10f0b571b9a6f8c9d18c8295c39698f3"
        },
        "date": 1795507626332,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 855.74070089,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1265.74967569,
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
          "id": "97eb8aa4f7b3e4e4d21f6a04772c21e6f202bbd8",
          "message": "workspace: upgrade cairo compiler to v2.22.0-rc.0 (#15317)\n\n* workspace: upgrade cairo compiler to v2.22.0-rc.0\n\nAlso bumps cairo-native to 0.9.0-rc.10 and moves proving to 7b3c99b\n(starkware-libs/proving#11 on sharp-8.0: only relaxes its cairo-lang requirement\nto 2.22.0-rc.0, so the prover and verifier are unchanged).\n\n- salsa is pinned to 0.28.2 in Cargo.lock, matching cairo's own lockfile:\n  cairo-lang-semantic 2.22.0-rc.0 does not compile against salsa 0.28.6.\n- The new Sierra version is 1.10.0. allowed_libfuncs.json mirrors cairo's\n  audited list: enum_match_v2 is allowed from 1.10, and enum_match,\n  enum_snapshot_match, enum_boxed_match, the *_const libfuncs and the old u96\n  guarantee checks are deprecated from 1.10.\n- The feature contracts import meta_tx_v0_syscall from corelib instead of\n  redeclaring it, since 2.22 rejects extern functions outside the corelib.\n- Regenerates the fixtures affected by the recompiled contracts.\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\n\n* apollo_gateway_config: raise the default max_sierra_version to 1.10\n\nClasses compiled with cairo v2.22 are Sierra 1.10, which the default range\n(up to 1.9) rejected on declare.\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\n\n* apollo_deployments: accept only Sierra 1.10 classes in the deployed gateways\n\nSierra 1.10 is produced by cairo v2.22, whose enum matches validate the variant\nindex before jumping on it. With min and max set to 1.10.0, new declares can't\ncontain the exploitable pattern.\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\n\n* workspace: pin proving to the merged cairo-lang bump\n\nstarkware-libs/proving-dev#692 merged into sharp-8.0 as 24d405e3 (mirrored to\nstarkware-libs/proving). Its Cargo files are identical to 7b3c99b; the merge\nonly adds CI workflow fixes.\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\n\n---------\n\nCo-authored-by: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-11-24T10:30:34Z",
          "tree_id": "4e79b3913ea0697597b688da9ec4d6c8e3c3ee42",
          "url": "https://github.com/starkware-libs/sequencer/commit/97eb8aa4f7b3e4e4d21f6a04772c21e6f202bbd8"
        },
        "date": 1795518597335,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 882.09704622,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1339.21920129,
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
          "id": "38ff83bf3baa20e5e9a3e4060efd0015d11c91c4",
          "message": "blockifier,starknet_api: temporarily pin salsa to 0.28.2 (#15319)\n\ncairo-lang 2.22.0-rc.0 requires salsa ^0.28.2, but cairo-lang-semantic doesn't\ncompile against salsa 0.28.6. Our Cargo.lock already pins 0.28.2, but users of\nthe published crates resolve their own versions, so the pin has to be in the\ncrates' manifests. Remove once cairo-lang fixes its salsa requirement.\n\nCo-authored-by: Claude Opus 5.5 <noreply@anthropic.com>",
          "timestamp": "2026-11-24T11:37:10Z",
          "tree_id": "5d9c1043fe82ad1f9a1800a858727c7720009bc0",
          "url": "https://github.com/starkware-libs/sequencer/commit/38ff83bf3baa20e5e9a3e4060efd0015d11c91c4"
        },
        "date": 1795522124231,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 858.26045125,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1324.7540588099998,
            "unit": "ms"
          }
        ]
      }
    ]
  }
}