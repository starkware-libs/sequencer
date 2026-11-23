window.BENCHMARK_DATA = {
  "lastUpdate": 1795433535155,
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
      }
    ]
  }
}