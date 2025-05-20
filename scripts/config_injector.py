import json
import os
import sys
from pathlib import Path


def load_json(path):
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


def save_json(data, path):
    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2)


def main(deployment_config_path: str, secrets_json_str: str):
    # Load deployment config
    with open(deployment_config_path, "r", encoding="utf-8") as f:
        deployment_config = json.load(f)

    # Get application config subdirectory
    config_dir = deployment_config["application_config_subdir"]
    config_dir_path = Path(os.environ["GITHUB_WORKSPACE"]) / config_dir
    config_dir_path.mkdir(parents=True, exist_ok=True)

    # Flatten all config file paths
    services = deployment_config.get("services", [])
    config_files = [
        cfg for service in services for cfg in service.get("config_paths", [])
    ]

    # Load the secrets JSON
    try:
        secrets = json.loads(secrets_json_str)
    except json.JSONDecodeError as e:
        print(f"❌ Failed to decode secrets JSON: {e}")
        sys.exit(1)

    # Inject secrets into each config
    for cfg_file in config_files:
        full_path = config_dir_path / cfg_file
        if not full_path.is_file():
            print(
                f"❌ Config file {full_path} not found. Available files in {config_dir_path}:"
            )
            for file in config_dir_path.iterdir():
                print(" -", file.name)
            sys.exit(1)

        print(f"💡 Injecting secrets into {full_path}")
        try:
            config_data = load_json(full_path)
            merged = {**config_data, **secrets}
            save_json(merged, full_path)
            print(f"✅ Injected secrets into {full_path}")
            print(full_path.read_text())
        except Exception as e:
            print(f"❌ Error processing {full_path}: {e}")
            sys.exit(1)

    print("✅ All configs updated successfully.")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print(
            "Usage: python inject_config_secrets.py <deployment_config_path> <secrets_json_env_var_name>"
        )
        sys.exit(1)

    config_path_arg = sys.argv[1]
    secrets_json = sys.argv[2]

    main(config_path_arg, secrets_json)
