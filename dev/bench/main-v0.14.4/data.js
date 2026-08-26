window.BENCHMARK_DATA = {
  "lastUpdate": 1787740691721,
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
      }
    ]
  }
}