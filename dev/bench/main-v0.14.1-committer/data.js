window.BENCHMARK_DATA = {
  "lastUpdate": 1769090691130,
  "repoUrl": "https://github.com/starkware-libs/sequencer",
  "entries": {
    "Benchmark": [
      {
        "commit": {
          "author": {
            "email": "56217775+dan-starkware@users.noreply.github.com",
            "name": "dan-starkware",
            "username": "dan-starkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "3d2a11993822655508057a4938bc2b7a03a7cb73",
          "message": "ci: add benchmark publish workflow for GitHub Pages (#11766)",
          "timestamp": "2026-01-22T11:59:57Z",
          "tree_id": "743e5b4400bc4e1ff5a1b7bdda84350af98ce448",
          "url": "https://github.com/starkware-libs/sequencer/commit/3d2a11993822655508057a4938bc2b7a03a7cb73"
        },
        "date": 1769084941276,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 867.00235505,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1331.8139557,
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
          "id": "1345cffcfb56c80f66ee513be1dd85208ae2ebf5",
          "message": "apollo_committer: replace CommitterStorageConfig with db_path field (#11856)\n\n- Replace nested storage_config: CommitterStorageConfig with direct db_path: PathBuf in CommitterConfig\n- Remove CommitterStorageConfig struct entirely\n- Update StorageConstructor trait to accept PathBuf instead of CommitterStorageConfig\n- Update all usages throughout codebase and integration tests\n- Update JSON config files to use committer_config.db_path instead of committer_config.storage_config.path\n- Regenerate config_schema.json with updated structure\n- Rename committer_storage_config field to committer_db_path in StorageTestConfig",
          "timestamp": "2026-01-22T12:20:29Z",
          "tree_id": "4797324dfc273c69180bd8ce192ad040aecb6210",
          "url": "https://github.com/starkware-libs/sequencer/commit/1345cffcfb56c80f66ee513be1dd85208ae2ebf5"
        },
        "date": 1769086455231,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 809.9883352200001,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1252.49538478,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "rotem@starkware.co",
            "name": "rotem-starkware",
            "username": "rotem-starkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "3eb7e5f53e680f084287851d7ef82bae415b80ee",
          "message": "starknet_committer: move BenchmarkTimeMeasurement to starknet_committer_cli (#11607)",
          "timestamp": "2026-01-22T12:29:34Z",
          "tree_id": "2a075fd20a711ee5fa218d08bf56cecbe746f094",
          "url": "https://github.com/starkware-libs/sequencer/commit/3eb7e5f53e680f084287851d7ef82bae415b80ee"
        },
        "date": 1769087257590,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 859.32378412,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1388.48199098,
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
          "distinct": false,
          "id": "2a42d11469c00d9964c0f0b34042e42b054f5293",
          "message": "starknet_patricia_storage: fix CSV stats columns in CachedStorage (#11896)\n\nSigned-off-by: Dori Medini <dori@starkware.co>",
          "timestamp": "2026-01-22T12:51:42Z",
          "tree_id": "aaa5d1034665c1d0dcee59dcb5c0dd5722e790c4",
          "url": "https://github.com/starkware-libs/sequencer/commit/2a42d11469c00d9964c0f0b34042e42b054f5293"
        },
        "date": 1769088098313,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 935.4181202799999,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1277.49075618,
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
          "id": "9feb0065af0e3cdfd47310ca953e746e453058c9",
          "message": "starknet_patricia_storage: enhance storage trait with EmptyStorageConfig (#11857)\n\n- Introduced EmptyStorageConfig struct for storage implementations that do not require configuration.\n- Updated Storage trait to include a Config associated type, defaulting to EmptyStorageConfig for various storage implementations (NullStorage, MapStorage, MdbxStorage, RocksDbStorage, AerospikeStorage, ShortKeyStorage).\n- Added necessary dependencies in Cargo.toml for apollo_config and validator.",
          "timestamp": "2026-01-22T13:35:35Z",
          "tree_id": "d9bf29f5d784323c990fede6687d80f86db863e1",
          "url": "https://github.com/starkware-libs/sequencer/commit/9feb0065af0e3cdfd47310ca953e746e453058c9"
        },
        "date": 1769090690785,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 816.48741884,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1383.41936723,
            "unit": "ms"
          }
        ]
      }
    ]
  }
}