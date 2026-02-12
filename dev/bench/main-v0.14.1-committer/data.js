window.BENCHMARK_DATA = {
  "lastUpdate": 1770895124442,
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
      },
      {
        "commit": {
          "author": {
            "email": "160594433+victorkstarkware@users.noreply.github.com",
            "name": "victorkstarkware",
            "username": "victorkstarkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7a5c38d5ad8482b38cee835b0c5dc3e9afefae6d",
          "message": "apollo_infra: migrate hyper to 1.x PART 4b server (#11797)",
          "timestamp": "2026-01-26T09:49:21Z",
          "tree_id": "b86149c16ea1ee0afe683d158645b6af529e4ec9",
          "url": "https://github.com/starkware-libs/sequencer/commit/7a5c38d5ad8482b38cee835b0c5dc3e9afefae6d"
        },
        "date": 1769422548000,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 804.92932434,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1318.66068456,
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
          "id": "8bc16ac41d6320a745718d961593fb226b1f437b",
          "message": "apollo_staking: split the `StakingMangerConfig` into dynamic and static configs (#11780)",
          "timestamp": "2026-01-26T12:46:11Z",
          "tree_id": "ff31988762c512ae610ab285dace5d189c1be0c9",
          "url": "https://github.com/starkware-libs/sequencer/commit/8bc16ac41d6320a745718d961593fb226b1f437b"
        },
        "date": 1769433198928,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 850.7849674600001,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1313.73852222,
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
          "id": "54bc66317c9950f8a7068e23936b8f1ca00b6379",
          "message": "starknet_committer: add ForestStorage initializer trait (#11850)",
          "timestamp": "2026-01-26T14:08:59Z",
          "tree_id": "f0f4211d71ecb4eca3516ec32951e30fc7862b8c",
          "url": "https://github.com/starkware-libs/sequencer/commit/54bc66317c9950f8a7068e23936b8f1ca00b6379"
        },
        "date": 1769438388652,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 906.995154,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1342.53262567,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "160594433+victorkstarkware@users.noreply.github.com",
            "name": "victorkstarkware",
            "username": "victorkstarkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "2680f0bf2c8bc05599c0990a7702e9902d1c486e",
          "message": "apollo_rpc: migrate hyper to 1.x PART 5 (#11904)",
          "timestamp": "2026-01-26T14:42:38Z",
          "tree_id": "1cff363c661ff38a31917de2f1403d5525278ec2",
          "url": "https://github.com/starkware-libs/sequencer/commit/2680f0bf2c8bc05599c0990a7702e9902d1c486e"
        },
        "date": 1769440510596,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 906.94978432,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1387.48669099,
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
          "distinct": true,
          "id": "b37cf1050413c2b01fced7c1eabf654ca08cca65",
          "message": "apollo_proc_macros: add unique id (#11897)",
          "timestamp": "2026-01-26T15:53:38Z",
          "tree_id": "69959abc21857a326e23073839543674a2ff7bfa",
          "url": "https://github.com/starkware-libs/sequencer/commit/b37cf1050413c2b01fced7c1eabf654ca08cca65"
        },
        "date": 1769444746600,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 930.00271705,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1292.29200338,
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
          "id": "3df44fb81d038d111e59317fbfa41628771f5bab",
          "message": "apollo_infra: use unique ports macro (#11902)",
          "timestamp": "2026-01-27T08:47:43Z",
          "tree_id": "715d6009930825ad972a5ac8a107d548b3836487",
          "url": "https://github.com/starkware-libs/sequencer/commit/3df44fb81d038d111e59317fbfa41628771f5bab"
        },
        "date": 1769505398594,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 861.8562945900001,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1308.36599233,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "160594433+victorkstarkware@users.noreply.github.com",
            "name": "victorkstarkware",
            "username": "victorkstarkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "1f81f0f73f5da79e41c504acce70159a4f49326e",
          "message": "apollo_integration_tests: migrate hyper to 1.x PART 6 (#12014)",
          "timestamp": "2026-01-27T09:49:52Z",
          "tree_id": "a61504cadeb2aecbf84ecb04201fa1e2f3ce7f20",
          "url": "https://github.com/starkware-libs/sequencer/commit/1f81f0f73f5da79e41c504acce70159a4f49326e"
        },
        "date": 1769509265631,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 872.26307682,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1310.91002403,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "160594433+victorkstarkware@users.noreply.github.com",
            "name": "victorkstarkware",
            "username": "victorkstarkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "735f97239a2ca3afc453ccd6fd75a2c6213f2960",
          "message": "apollo_gateway: migrate hyper to 1.x PART 8 (#12012)",
          "timestamp": "2026-01-27T09:58:47Z",
          "tree_id": "dcafc8b2335a3a6656aad9de75522b6dc487024f",
          "url": "https://github.com/starkware-libs/sequencer/commit/735f97239a2ca3afc453ccd6fd75a2c6213f2960"
        },
        "date": 1769510216643,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 799.34609148,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1208.42268925,
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
          "id": "7d407c9e1fe9e7068c80614ed46b24340e63ec20",
          "message": "apollo_staking: mock staking contract Rust implementation (#12044)",
          "timestamp": "2026-01-27T10:36:47Z",
          "tree_id": "5dfb0ccec6ffe09027f047d547ef5766a687c516",
          "url": "https://github.com/starkware-libs/sequencer/commit/7d407c9e1fe9e7068c80614ed46b24340e63ec20"
        },
        "date": 1769511890620,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 822.53124083,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1251.1566292,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "andrew.l@starkware.co",
            "name": "Andrew Luka",
            "username": "sirandreww-starkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "5d1479852c68a9f10c0dfdc8375f5d2ed626558f",
          "message": "apollo_propeller: add prost codec for length-delimited messages (#11063)",
          "timestamp": "2026-01-27T12:41:29Z",
          "tree_id": "5804fd855dbbebc71cfc53cbe412b2ff61f8be1b",
          "url": "https://github.com/starkware-libs/sequencer/commit/5d1479852c68a9f10c0dfdc8375f5d2ed626558f"
        },
        "date": 1769519453011,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 926.8754372100001,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1452.88163154,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "163830216+asmaastarkware@users.noreply.github.com",
            "name": "asmaa-starkware",
            "username": "asmaastarkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "6a4298fbd63de48831234a7e5b5b582c4d949d4e",
          "message": "apollo_protobuf: add signature field to Vote consensus message (#11938)",
          "timestamp": "2026-01-27T13:19:12Z",
          "tree_id": "9db78a8ca754230968887708d8a5e23d3fa11352",
          "url": "https://github.com/starkware-libs/sequencer/commit/6a4298fbd63de48831234a7e5b5b582c4d949d4e"
        },
        "date": 1769521962142,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 1089.9700703800002,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1407.9191233699999,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "andrew.l@starkware.co",
            "name": "Andrew Luka",
            "username": "sirandreww-starkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "7805f2d079e35fd06b1733226fbbff55672b1453",
          "message": "apollo_propeller: add protocol upgrade for libp2p streams (#11064)",
          "timestamp": "2026-01-27T14:01:31Z",
          "tree_id": "7c0075a3490610703838e377063b9b97a982e24a",
          "url": "https://github.com/starkware-libs/sequencer/commit/7805f2d079e35fd06b1733226fbbff55672b1453"
        },
        "date": 1769524358302,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 989.7129360900001,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1431.21059604,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "160594433+victorkstarkware@users.noreply.github.com",
            "name": "victorkstarkware",
            "username": "victorkstarkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "325ded0aa7f893805b2f7000dbf44827ea08553a",
          "message": "apollo_storage: migrate hyper to 1.x PART 7 (#11994)",
          "timestamp": "2026-01-27T15:09:17Z",
          "tree_id": "6bcb83540956607d50bd79fde7139e51f15779cc",
          "url": "https://github.com/starkware-libs/sequencer/commit/325ded0aa7f893805b2f7000dbf44827ea08553a"
        },
        "date": 1769528608302,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 1081.3830554,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1445.4610174200002,
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
          "id": "2e48adfe3c9b3fe90808c8e5f23e60d06234f3b3",
          "message": "starknet_committer,apollo_node: initialize apollo committer with index db (#11841)",
          "timestamp": "2026-01-27T18:24:16Z",
          "tree_id": "5c1a45529e214841cf02699ed61c104ac43c8e53",
          "url": "https://github.com/starkware-libs/sequencer/commit/2e48adfe3c9b3fe90808c8e5f23e60d06234f3b3"
        },
        "date": 1769540228828,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 925.23703734,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1408.21425796,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "163830216+asmaastarkware@users.noreply.github.com",
            "name": "asmaa-starkware",
            "username": "asmaastarkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "185affdd11b5185dec7d5e8dc3d496cc25b86760",
          "message": "apollo_consensus_orchestrator: add optional CommitteeProvider to context deps (#12020)",
          "timestamp": "2026-01-28T10:03:12Z",
          "tree_id": "795a000ae178be7a43a2f4bda8d5fc5b9dd9e0e9",
          "url": "https://github.com/starkware-libs/sequencer/commit/185affdd11b5185dec7d5e8dc3d496cc25b86760"
        },
        "date": 1769596263367,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 856.43239811,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1287.5433476199998,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "160594433+victorkstarkware@users.noreply.github.com",
            "name": "victorkstarkware",
            "username": "victorkstarkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "2e2cf66824c9e754c337e7b92e95dea0eb2f2072",
          "message": "apollo_batcher_types,apollo_monitoring_endpoint: migrate hyper to 1.x PART 9 (#12054)",
          "timestamp": "2026-01-28T11:40:08Z",
          "tree_id": "0e4687319302c1e0ac87589bbd6eba665001830c",
          "url": "https://github.com/starkware-libs/sequencer/commit/2e2cf66824c9e754c337e7b92e95dea0eb2f2072"
        },
        "date": 1769602050003,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 847.49097579,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1323.4946213699998,
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
          "id": "abaecdaf4a5e579893cd9e7e4db2632196b95f79",
          "message": "starknet_committer: rename timing_util and TimeMeasurementTrait and structs (#11970)",
          "timestamp": "2026-01-28T12:07:17Z",
          "tree_id": "1bb7a7b403668f3aca5a99ad6977f58452cc7bfa",
          "url": "https://github.com/starkware-libs/sequencer/commit/abaecdaf4a5e579893cd9e7e4db2632196b95f79"
        },
        "date": 1769603818013,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 861.67102919,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1300.3800959,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "160594433+victorkstarkware@users.noreply.github.com",
            "name": "victorkstarkware",
            "username": "victorkstarkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "2e942e2a852873380c0c150e7025145db4d74a66",
          "message": "apollo_storage,apollo_rpc: migrate hyper to 1.x PART 10 (#12074)",
          "timestamp": "2026-01-28T12:32:35Z",
          "tree_id": "845639544618d912569d8d0d6a7b2af8fcb6c980",
          "url": "https://github.com/starkware-libs/sequencer/commit/2e942e2a852873380c0c150e7025145db4d74a66"
        },
        "date": 1769605223049,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 860.50101489,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1303.11844157,
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
          "id": "545dccc9187cb63938daffbe9f15ead90f0a3227",
          "message": "starknet_committer: define BlockDurations and use it in BlockMeasurement (#11971)",
          "timestamp": "2026-01-28T12:43:32Z",
          "tree_id": "94348cc06e8fa8c3f0799e7183d9f8d1e45a3092",
          "url": "https://github.com/starkware-libs/sequencer/commit/545dccc9187cb63938daffbe9f15ead90f0a3227"
        },
        "date": 1769606037568,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 832.55302959,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1454.33997674,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "160594433+victorkstarkware@users.noreply.github.com",
            "name": "victorkstarkware",
            "username": "victorkstarkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "c5dc5f18ef8c3e19ec1cf1a7609c0fcfcb26be01",
          "message": "workspace: migrate hyper to 1.x PART 11 (#12075)",
          "timestamp": "2026-01-28T13:34:46Z",
          "tree_id": "60d9687a741b0765399473367d868f0a1a939cc8",
          "url": "https://github.com/starkware-libs/sequencer/commit/c5dc5f18ef8c3e19ec1cf1a7609c0fcfcb26be01"
        },
        "date": 1769610987560,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 904.9715361,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1320.32154084,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "160594433+victorkstarkware@users.noreply.github.com",
            "name": "victorkstarkware",
            "username": "victorkstarkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "c5dc5f18ef8c3e19ec1cf1a7609c0fcfcb26be01",
          "message": "workspace: migrate hyper to 1.x PART 11 (#12075)",
          "timestamp": "2026-01-28T13:34:46Z",
          "tree_id": "60d9687a741b0765399473367d868f0a1a939cc8",
          "url": "https://github.com/starkware-libs/sequencer/commit/c5dc5f18ef8c3e19ec1cf1a7609c0fcfcb26be01"
        },
        "date": 1769611951704,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 791.2267393,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1234.06711169,
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
          "distinct": true,
          "id": "010e55d01e766e628a539b256881bbb6e36eb16a",
          "message": "starknet_committer: add number of modifications to BlockMeasurement and impl set function (#11917)",
          "timestamp": "2026-01-29T08:50:09Z",
          "tree_id": "c9b355039da5a9d92839c1e65eddc51bf56d1985",
          "url": "https://github.com/starkware-libs/sequencer/commit/010e55d01e766e628a539b256881bbb6e36eb16a"
        },
        "date": 1769678240466,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 751.5030739700001,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1177.14540695,
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
          "id": "05aad19fc5ea58c6f4af775ca7d0f95559c4e6cb",
          "message": "apollo_batcher: full revert commitment flow (#11496)",
          "timestamp": "2026-01-29T09:21:05Z",
          "tree_id": "385941d0aa67adb0b4b1ac697a0239848784dde5",
          "url": "https://github.com/starkware-libs/sequencer/commit/05aad19fc5ea58c6f4af775ca7d0f95559c4e6cb"
        },
        "date": 1769680156018,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 808.27237292,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1174.60667503,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "163830216+asmaastarkware@users.noreply.github.com",
            "name": "asmaa-starkware",
            "username": "asmaastarkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "bc395bab2976d340b1c87393f9f2edf752a777d8",
          "message": "apollo_staking: implement deterministic pseudorandom generator (#12097)",
          "timestamp": "2026-01-29T13:47:53Z",
          "tree_id": "5808fdbd52bf4e633602dfe6664e242b26b83c5d",
          "url": "https://github.com/starkware-libs/sequencer/commit/bc395bab2976d340b1c87393f9f2edf752a777d8"
        },
        "date": 1769696418729,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 799.52182576,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1301.2830254300002,
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
          "id": "2d1ba135f3ab674ed9a7ed39a0771de4c5d9b26e",
          "message": "starknet_committer_cli,starknet_patricia_storage: generic CachedStorage config (#12047)",
          "timestamp": "2026-01-29T15:41:08Z",
          "tree_id": "2ff8bef6e91d97341daa97df76b94b9385efb431",
          "url": "https://github.com/starkware-libs/sequencer/commit/2d1ba135f3ab674ed9a7ed39a0771de4c5d9b26e"
        },
        "date": 1769703148375,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 877.7789124,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1374.11525789,
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
          "id": "e1711969bf8bafc5925968bcd84bd9afba5ecb14",
          "message": "starknet_committer: add number of empty leaves to BlockModificationsCounts (#11919)",
          "timestamp": "2026-01-29T17:57:24Z",
          "tree_id": "ae43f74561c41e71dbfb009cf03080ee877b30c8",
          "url": "https://github.com/starkware-libs/sequencer/commit/e1711969bf8bafc5925968bcd84bd9afba5ecb14"
        },
        "date": 1769711516712,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 893.0606316699999,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1318.50253451,
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
          "id": "a7137509abaa9121b1eacd739edb5ae422286480",
          "message": "apollo_committer_types: extend mock committer client (#12115)",
          "timestamp": "2026-02-01T08:25:27Z",
          "tree_id": "327515006346b78a6876d51d66a6238e73601547",
          "url": "https://github.com/starkware-libs/sequencer/commit/a7137509abaa9121b1eacd739edb5ae422286480"
        },
        "date": 1769935894312,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 820.7128976399999,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1223.5605400999998,
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
          "id": "5897571e91c05c439d6059a859ff436a39da8f74",
          "message": "starknet_committer,starknet_patricia_storage: rocksdb storage config (#12048)",
          "timestamp": "2026-02-01T10:29:44Z",
          "tree_id": "48051901e10fe9289fcfa821bce3bea9ac44ecac",
          "url": "https://github.com/starkware-libs/sequencer/commit/5897571e91c05c439d6059a859ff436a39da8f74"
        },
        "date": 1769943419339,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 743.17701129,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1144.41383524,
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
          "id": "485a7e13bd498339546b50ea7b67a311dec51750",
          "message": "starknet_committer: add original tree creation test templates (#11715)",
          "timestamp": "2026-02-01T11:42:37Z",
          "tree_id": "f80558a5164326f9835ceeb850f38e02050b66a4",
          "url": "https://github.com/starkware-libs/sequencer/commit/485a7e13bd498339546b50ea7b67a311dec51750"
        },
        "date": 1769947760329,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 774.0848544500001,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1157.79645825,
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
          "distinct": true,
          "id": "a1b6b125e189a9a4f1019f25e3a9430ca064a3bc",
          "message": "starknet_committer: change BlockDurations and committer durations metrics to be in seconds (#12183)",
          "timestamp": "2026-02-01T17:44:10Z",
          "tree_id": "154044e4e7205dfccf2bfc708b0d750a7973096e",
          "url": "https://github.com/starkware-libs/sequencer/commit/a1b6b125e189a9a4f1019f25e3a9430ca064a3bc"
        },
        "date": 1769969386277,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 745.0746668400001,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1144.2724621099999,
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
          "id": "658dbb336244597bdd9d4a40793fd37d99b2d274",
          "message": "starknet_committer,starknet_patricia: layout-based original tree creation tests (#11236)",
          "timestamp": "2026-02-02T05:04:29Z",
          "tree_id": "6e5b8b46d48553fae43f0fa6c4d3f689ef8d2741",
          "url": "https://github.com/starkware-libs/sequencer/commit/658dbb336244597bdd9d4a40793fd37d99b2d274"
        },
        "date": 1770010414356,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 894.50318663,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1326.8248468699999,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "88497213+amosStarkware@users.noreply.github.com",
            "name": "amosStarkware",
            "username": "amosStarkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "5ac6b3990bccf706be9e18cc7203e6e2455961ee",
          "message": "starknet_patricia_storage: make aerospike commit level explicit (#11745)",
          "timestamp": "2026-02-02T08:21:47Z",
          "tree_id": "76fdf5430c4b5c4a700a5e0cbafd75a2d2ec9ce1",
          "url": "https://github.com/starkware-libs/sequencer/commit/5ac6b3990bccf706be9e18cc7203e6e2455961ee"
        },
        "date": 1770022351197,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 829.18088571,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1244.89634771,
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
          "id": "05cdefc9b171ab47938af072215184e11b46d66c",
          "message": "starknet_committer: generic hash in index layout (#12223)",
          "timestamp": "2026-02-03T10:27:26Z",
          "tree_id": "05472f1389f2249b57668618ae97953b81f78dd2",
          "url": "https://github.com/starkware-libs/sequencer/commit/05cdefc9b171ab47938af072215184e11b46d66c"
        },
        "date": 1770115942877,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 817.49810253,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1261.31889908,
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
          "id": "c76ddee5b3e2ecd876ee45e9bfcb037cb85f7198",
          "message": "starknet_committer: mock tree hash function (#12224)",
          "timestamp": "2026-02-03T11:51:45Z",
          "tree_id": "70efc48fd870be9e85652aba35c20b1b8192ec67",
          "url": "https://github.com/starkware-libs/sequencer/commit/c76ddee5b3e2ecd876ee45e9bfcb037cb85f7198"
        },
        "date": 1770120398924,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 783.5420098200001,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1237.76033571,
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
          "id": "aec17db08e9b6e7cf51dd5c93aee766a8d1b00c1",
          "message": "starknet_patricia_storage: remove dummy field from EmptyStorageConfig (#12218)",
          "timestamp": "2026-02-03T12:55:56Z",
          "tree_id": "8ca8dc5f993b9f67bb8d5ee9f8be875ea5c8742a",
          "url": "https://github.com/starkware-libs/sequencer/commit/aec17db08e9b6e7cf51dd5c93aee766a8d1b00c1"
        },
        "date": 1770124607610,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 897.6326981699999,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1287.1884689600001,
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
          "id": "d9406f415a7ba18407eade4a0db982a5c82c298f",
          "message": "starknet_committer: make FactsDb storage field private (#12221)",
          "timestamp": "2026-02-03T12:56:07Z",
          "tree_id": "c970c5ee1d45403ac5da5d7a734796f273f85820",
          "url": "https://github.com/starkware-libs/sequencer/commit/d9406f415a7ba18407eade4a0db982a5c82c298f"
        },
        "date": 1770125080900,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 812.5014711599999,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1255.1016317,
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
          "id": "c3b8e1ff3efb986012d1050b1e508b6f490828fd",
          "message": "index layout forest creation tests (#11286)",
          "timestamp": "2026-02-03T14:11:55Z",
          "tree_id": "06d01dd74eefc717be7b456dd9a8b19c7465b849",
          "url": "https://github.com/starkware-libs/sequencer/commit/c3b8e1ff3efb986012d1050b1e508b6f490828fd"
        },
        "date": 1770129468772,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 889.32184992,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1362.51489054,
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
          "id": "d4e1ee95c44182e10c66da8bdc06acffa8caaf5a",
          "message": "Merge pull request #12234 from starkware-libs/dori/merge-main-v0.14.1-into-main-v0.14.1-committer-1770110377\n\nMerge main-v0.14.1 into main-v0.14.1-committer",
          "timestamp": "2026-02-03T16:09:43Z",
          "tree_id": "6bf6501d5e8d16859d5c92fbc443e9fd0083b6b5",
          "url": "https://github.com/starkware-libs/sequencer/commit/d4e1ee95c44182e10c66da8bdc06acffa8caaf5a"
        },
        "date": 1770138713738,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 999.21916727,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1351.5227614100002,
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
          "id": "2ace6a445ab09da90f0110c2574a927e88e990c7",
          "message": "starknet_committer: add commit e2e tests (#11331)",
          "timestamp": "2026-02-04T08:38:27Z",
          "tree_id": "2a28bccbf372c229af1a2c4ad3555d462c16d093",
          "url": "https://github.com/starkware-libs/sequencer/commit/2ace6a445ab09da90f0110c2574a927e88e990c7"
        },
        "date": 1770195637580,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 742.95480818,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1173.6307531700002,
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
          "id": "dcc94ff61dd17b90dfb8ab85c8a005c2b49ab2a7",
          "message": "apollo_dashboard: add committer dashboard (#12051)",
          "timestamp": "2026-02-04T10:35:26Z",
          "tree_id": "6a354c6c75f69a46658c9e5ec81eb72ea4a89c37",
          "url": "https://github.com/starkware-libs/sequencer/commit/dcc94ff61dd17b90dfb8ab85c8a005c2b49ab2a7"
        },
        "date": 1770202412767,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 756.0158566599999,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1194.96899439,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "163830216+asmaastarkware@users.noreply.github.com",
            "name": "asmaa-starkware",
            "username": "asmaastarkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "e6d705364175f5d69c30ae6b6e5f4e888fd37dd9",
          "message": "apollo_staking: add ProposerLookup trait, and EpochCommittee (#12291)",
          "timestamp": "2026-02-04T13:49:36Z",
          "tree_id": "447106c4151aed6a35004b9286d2f8c77a6cfdcc",
          "url": "https://github.com/starkware-libs/sequencer/commit/e6d705364175f5d69c30ae6b6e5f4e888fd37dd9"
        },
        "date": 1770214399205,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 1056.66336685,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1425.91083124,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "163830216+asmaastarkware@users.noreply.github.com",
            "name": "asmaa-starkware",
            "username": "asmaastarkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ccd065674d668a249b703bae023acb294d14eb4b",
          "message": "Revert \"apollo_staking: add ProposerLookup trait, and EpochCommittee (#12291)\" (#12295)\n\nThis reverts commit e6d705364175f5d69c30ae6b6e5f4e888fd37dd9.",
          "timestamp": "2026-02-04T15:59:18Z",
          "tree_id": "9d6168b7a76bdd96fe5a425e0ebd78b4e0c78973",
          "url": "https://github.com/starkware-libs/sequencer/commit/ccd065674d668a249b703bae023acb294d14eb4b"
        },
        "date": 1770222057053,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 840.20555089,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1398.485825,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "93648739+nadin-Starkware@users.noreply.github.com",
            "name": "Nadin Jbara",
            "username": "nadin-Starkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f580233db9538e2c1bbdf0d2b76e9645edf838f3",
          "message": "apollo_config_manager: Add batcher dynamic config to config manager (#12281)",
          "timestamp": "2026-02-05T08:25:17Z",
          "tree_id": "d19766ee12f57ff109bc6305a35ac74d701d1307",
          "url": "https://github.com/starkware-libs/sequencer/commit/f580233db9538e2c1bbdf0d2b76e9645edf838f3"
        },
        "date": 1770281393488,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 950.14278463,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1355.9708716199998,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "93648739+nadin-Starkware@users.noreply.github.com",
            "name": "Nadin Jbara",
            "username": "nadin-Starkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "177003518a4794481dd8c16251db671c5efc64c9",
          "message": "apollo_batcher,apollo_config_manager: wire up batcher dynamic config updates (#12292)",
          "timestamp": "2026-02-05T09:46:33Z",
          "tree_id": "64db9abba3a93022079452760d0159a46ea5d552",
          "url": "https://github.com/starkware-libs/sequencer/commit/177003518a4794481dd8c16251db671c5efc64c9"
        },
        "date": 1770285928813,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 851.69974266,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1333.57413024,
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
          "id": "0cf19788bcaf5ce9e8862e0809405798c114c9be",
          "message": "starknet_committer: reorganize db modules (#11640)",
          "timestamp": "2026-02-05T11:44:54Z",
          "tree_id": "ec3206cd7fc9d3b4a55a950d924561783f3fd223",
          "url": "https://github.com/starkware-libs/sequencer/commit/0cf19788bcaf5ce9e8862e0809405798c114c9be"
        },
        "date": 1770293124674,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 787.29715385,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1224.11235873,
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
          "id": "97a0b4454e1cb76959faf385fb7994cba23f86ab",
          "message": "starknet_committer,starknet_patricia: move node_serde to facts db (#11644)",
          "timestamp": "2026-02-05T14:24:24Z",
          "tree_id": "84ddfdd4f563b56588433066bd2111fa35c2d713",
          "url": "https://github.com/starkware-libs/sequencer/commit/97a0b4454e1cb76959faf385fb7994cba23f86ab"
        },
        "date": 1770302838705,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 887.2403140800001,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1338.68537746,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "163830216+asmaastarkware@users.noreply.github.com",
            "name": "asmaa-starkware",
            "username": "asmaastarkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "40685fcf42cce42c029dff86b7a8bd1af6a950e2",
          "message": "apollo_staking_config: impl serialization and deserialization for CommitteeConfig (#12315)",
          "timestamp": "2026-02-08T11:41:03Z",
          "tree_id": "272d362f3bb943642a434dbb191ddeffb26fd99f",
          "url": "https://github.com/starkware-libs/sequencer/commit/40685fcf42cce42c029dff86b7a8bd1af6a950e2"
        },
        "date": 1770551963500,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 852.7842640599999,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1203.26548471,
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
          "distinct": true,
          "id": "fde6c98f2c0368352d525f92e95b6aaf80191b6f",
          "message": "apollo_dashboard: remove duplicated per-env dashboard files (#12373)",
          "timestamp": "2026-02-09T18:17:45Z",
          "tree_id": "d3680014f51c5ed251387f64b3a0bf42f6a2280d",
          "url": "https://github.com/starkware-libs/sequencer/commit/fde6c98f2c0368352d525f92e95b6aaf80191b6f"
        },
        "date": 1770662489105,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 924.48856103,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1379.49321612,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "161198342+idan-starkware@users.noreply.github.com",
            "name": "Idan Shamam",
            "username": "idan-starkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "528be8ef1b4039fe1ad979cc8c1aa8c2fd02806f",
          "message": "deployment: create action composite for namespace cache on bootstrap (#12379)\n\n* deployment: create action composite for namespace cache on bootstrap\n\n* deployment: remove pip cache from tool chain setup action\n\n* deployment: remove swatinem rust cache\n\n* deployment: fix blockifier_ci bootstrap post job bug\n\n* deployment: stronger runners for demanding jobs\n\n* deployment: upgrade checkout action to v6",
          "timestamp": "2026-02-10T07:48:22Z",
          "tree_id": "1137258fa444a54af66c1fcb7c19fdd1c827dddb",
          "url": "https://github.com/starkware-libs/sequencer/commit/528be8ef1b4039fe1ad979cc8c1aa8c2fd02806f"
        },
        "date": 1770710897128,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 842.85148651,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1262.68984783,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "163830216+asmaastarkware@users.noreply.github.com",
            "name": "asmaa-starkware",
            "username": "asmaastarkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "4d76e07847510065b48d0e3c0c674a08c584ba52",
          "message": "apollo_consensus: wire CommitteeProvider into consensus and MultiHeightManager (#12397)",
          "timestamp": "2026-02-10T08:24:43Z",
          "tree_id": "1eb977d59eb55b4654c9ed7734ce4076adbaa349",
          "url": "https://github.com/starkware-libs/sequencer/commit/4d76e07847510065b48d0e3c0c674a08c584ba52"
        },
        "date": 1770713120201,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 860.95037446,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1341.46197584,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "ron@starkware.co",
            "name": "ron-starkware",
            "username": "ron-starkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0ff210983ca06c4ebd4829b6dba6876da5a0274e",
          "message": "apollo_dashboard: Fix Seconds since last function for Grafana panels (#11982)",
          "timestamp": "2026-02-10T08:28:25Z",
          "tree_id": "aee1ac51fcbc37a7be62f0239218fe7b0fc1ec89",
          "url": "https://github.com/starkware-libs/sequencer/commit/0ff210983ca06c4ebd4829b6dba6876da5a0274e"
        },
        "date": 1770714278339,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 779.97230573,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1320.66560407,
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
          "distinct": true,
          "id": "3c22a3760bca8b26be0d84f61f61957e2a4be787",
          "message": "starknet_api: add time measurements to calculate_block_commitments (#12414)",
          "timestamp": "2026-02-10T09:02:55Z",
          "tree_id": "47a1c3345e7971a483605c730b55367b00528851",
          "url": "https://github.com/starkware-libs/sequencer/commit/3c22a3760bca8b26be0d84f61f61957e2a4be787"
        },
        "date": 1770715428018,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 813.30527225,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1271.31343474,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "163830216+asmaastarkware@users.noreply.github.com",
            "name": "asmaa-starkware",
            "username": "asmaastarkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "dad53154c81c35135c18e2cded488ce6da8c19c7",
          "message": "apollo_consensus_orchestrator: delete committee provider from context deps (#12407)",
          "timestamp": "2026-02-10T13:49:47Z",
          "tree_id": "4a98611458a8d3d8165975a6a28c943fd9c167c3",
          "url": "https://github.com/starkware-libs/sequencer/commit/dad53154c81c35135c18e2cded488ce6da8c19c7"
        },
        "date": 1770732597684,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 915.24261907,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1349.16103627,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "arnon@starkware.co",
            "name": "Arnon Hod",
            "username": "ArniStarkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "8ce160afea0a2fac160890349df9a1eb462db5bd",
          "message": "apollo_http_server: add dynamic config size validation (#12438)",
          "timestamp": "2026-02-11T19:03:40Z",
          "tree_id": "c0ff1030e92bfc9f3c8cc9d6a5dbf5bb58b33aea",
          "url": "https://github.com/starkware-libs/sequencer/commit/8ce160afea0a2fac160890349df9a1eb462db5bd"
        },
        "date": 1770837888127,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 900.66674985,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1367.61755978,
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
          "id": "3d1f0ffead7e67bee8e8e89f344e724167ae2b7d",
          "message": "apollo_committer: add TOTAL_BLOCK_DURATION_PER_MODIFICATION metric and panel (#12449)",
          "timestamp": "2026-02-12T09:41:04Z",
          "tree_id": "7d42ed8107d8d5cee041f7d9e838f07d0088a823",
          "url": "https://github.com/starkware-libs/sequencer/commit/3d1f0ffead7e67bee8e8e89f344e724167ae2b7d"
        },
        "date": 1770890369309,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 812.09331351,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1290.77002465,
            "unit": "ms"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "138376632+ayeletstarkware@users.noreply.github.com",
            "name": "Ayelet Zilber",
            "username": "ayeletstarkware"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "d5830cfa888eeb53620512bbdd4d58dfe04c2e3a",
          "message": "apollo_consensus_orchestrator: add min gas price to fee market calculation (#12522)",
          "timestamp": "2026-02-12T10:59:46Z",
          "tree_id": "29a6de0a384c7a981f03cffbbac12a9f5ccd6b05",
          "url": "https://github.com/starkware-libs/sequencer/commit/d5830cfa888eeb53620512bbdd4d58dfe04c2e3a"
        },
        "date": 1770895124139,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "full_committer_flow",
            "value": 790.59533989,
            "unit": "ms"
          },
          {
            "name": "tree_computation_flow",
            "value": 1159.4549684,
            "unit": "ms"
          }
        ]
      }
    ]
  }
}