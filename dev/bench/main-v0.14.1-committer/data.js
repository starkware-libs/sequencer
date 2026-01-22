window.BENCHMARK_DATA = {
  "lastUpdate": 1769086455543,
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
      }
    ]
  }
}