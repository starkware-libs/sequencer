window.BENCHMARK_DATA = {
  "lastUpdate": 1769417019200,
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
      },
      {
        "commit": {
          "author": {
            "email": "106665835+Itay-Tsabary-Starkware@users.noreply.github.com",
            "name": "Itay-Tsabary-Starkware",
            "username": "Itay-Tsabary-Starkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "f5a4ae6d423beff2cf0bc39bd20b2652fff2a371",
          "message": "apollo_node: signal handling (#11894)",
          "timestamp": "2026-01-22T20:33:39Z",
          "tree_id": "352746a66ada3fe7740af3607231a31f3604ab8e",
          "url": "https://github.com/starkware-libs/sequencer/commit/f5a4ae6d423beff2cf0bc39bd20b2652fff2a371"
        },
        "date": 1769115907421,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 899.04739037,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1340.14108681,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "86294909+ArielElp@users.noreply.github.com",
            "name": "ArielElp",
            "username": "ArielElp"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "fee12a14d8fa393b6c70a5413d64bde33d4af1bc",
          "message": "starknet_committer,starknet_patricia_storage: index db read initial roots (#11839)",
          "timestamp": "2026-01-23T11:08:23Z",
          "tree_id": "541a6195fe3de999ba874e124809de75c086d794",
          "url": "https://github.com/starkware-libs/sequencer/commit/fee12a14d8fa393b6c70a5413d64bde33d4af1bc"
        },
        "date": 1769168389247,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 748.3094817,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1227.1022541,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "143319383+nimrod-starkware@users.noreply.github.com",
            "name": "nimrod-starkware",
            "username": "nimrod-starkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "b0a41e309772febd4a7c00118357f153b8af8f4c",
          "message": "starknet_patricia_storage: tune rocksDB config (#11874)",
          "timestamp": "2026-01-25T08:39:42Z",
          "tree_id": "ed511f48d40a36a6b8d9304b4ced2f2d7e8f7c16",
          "url": "https://github.com/starkware-libs/sequencer/commit/b0a41e309772febd4a7c00118357f153b8af8f4c"
        },
        "date": 1769332007656,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 739.98201298,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1137.52179677,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "86294909+ArielElp@users.noreply.github.com",
            "name": "ArielElp",
            "username": "ArielElp"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "17c683584bae34e2fa5a1abd58615d23cff5b20a",
          "message": "starknet_committer: index db metadata impl (#11840)\n\n* starknet_committer,starknet_patricia_storage: index db read initial roots\n\n* starknet_committer: index db metadata impl",
          "timestamp": "2026-01-25T08:53:06Z",
          "tree_id": "0a65858965d14d37197ab355ff515f8625a4bcf8",
          "url": "https://github.com/starkware-libs/sequencer/commit/17c683584bae34e2fa5a1abd58615d23cff5b20a"
        },
        "date": 1769332811412,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 764.67025663,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1250.7002146099999,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "143319383+nimrod-starkware@users.noreply.github.com",
            "name": "nimrod-starkware",
            "username": "nimrod-starkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ccd07db1d1727dfc28c9209cf1c9753728163e88",
          "message": "starknet_patricia_storage: save options (#11875)\n\n* starknet_patricia_storage: tune rocksDB config\n\n* starknet_patricia_storage: save options",
          "timestamp": "2026-01-25T09:15:20Z",
          "tree_id": "c248e552547ecef5a28ff84a6b0d29cc1981659e",
          "url": "https://github.com/starkware-libs/sequencer/commit/ccd07db1d1727dfc28c9209cf1c9753728163e88"
        },
        "date": 1769334111540,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 819.75251912,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1208.46151001,
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
          "id": "0a5ddc7ec832258bdc7a51f1ec19333460178c42",
          "message": "apollo_committer_config: add storage config to apollo_committer_config (#11858)",
          "timestamp": "2026-01-25T12:46:09Z",
          "tree_id": "a3c19d3a8b27c001409b73a90050ed0ed994ad9a",
          "url": "https://github.com/starkware-libs/sequencer/commit/0a5ddc7ec832258bdc7a51f1ec19333460178c42"
        },
        "date": 1769346780344,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 821.57126535,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1255.16215511,
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
          "id": "70d26f074af4c508a095cc5d91dc01b3e3ddd57a",
          "message": "starknet_patricia_storage: use fixed aerospike version (#11924)\n\nSigned-off-by: Dori Medini <dori@starkware.co>",
          "timestamp": "2026-01-25T13:58:01Z",
          "tree_id": "3dad42fd4889857a449bb56e20ad2df4e54ed0c8",
          "url": "https://github.com/starkware-libs/sequencer/commit/70d26f074af4c508a095cc5d91dc01b3e3ddd57a"
        },
        "date": 1769351129018,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 817.6031924199999,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1252.99139396,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "143319383+nimrod-starkware@users.noreply.github.com",
            "name": "nimrod-starkware",
            "username": "nimrod-starkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "080e95f08fb7a46c97454a6ca4ccd33051d01663",
          "message": "starknet_patricia_storage: rocksDB stats (#11876)",
          "timestamp": "2026-01-25T15:56:29Z",
          "tree_id": "5880552152c753b368394db3a845d9f50ce16c21",
          "url": "https://github.com/starkware-libs/sequencer/commit/080e95f08fb7a46c97454a6ca4ccd33051d01663"
        },
        "date": 1769358236023,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 794.4543970599999,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1168.94950674,
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
          "id": "15922d21b039976932d3e83f4dea2a997202deb0",
          "message": "apollo_committer: remove facts terminology from commit_block (#11915)",
          "timestamp": "2026-01-25T16:07:22Z",
          "tree_id": "4af9e9760b31acef86010c1f4b2957071802d26e",
          "url": "https://github.com/starkware-libs/sequencer/commit/15922d21b039976932d3e83f4dea2a997202deb0"
        },
        "date": 1769358942599,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 747.47147549,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1138.44255466,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "143319383+nimrod-starkware@users.noreply.github.com",
            "name": "nimrod-starkware",
            "username": "nimrod-starkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "b1dfc207628081177730144fdd32147be0267b2a",
          "message": "starknet_patricia_storage: use index DB for benchmarks (#11877)",
          "timestamp": "2026-01-26T08:15:07Z",
          "tree_id": "5749996dc5429d73826ff834878557edfba429fd",
          "url": "https://github.com/starkware-libs/sequencer/commit/b1dfc207628081177730144fdd32147be0267b2a"
        },
        "date": 1769417018909,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 781.42450596,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1242.31966543,
            "unit": "ms"
          }
        ]
      }
    ]
  }
}