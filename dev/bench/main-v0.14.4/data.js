window.BENCHMARK_DATA = {
  "lastUpdate": 1788080398984,
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
      }
    ]
  }
}