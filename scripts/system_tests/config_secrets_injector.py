import argparse
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

    deployment_config = load_json(deployment_config_path)

    # Get application config subdirectory
    config_dir = deployment_config["application_config_subdir"]
    config_dir_path = Path(os.environ["GITHUB_WORKSPACE"]) / config_dir

    # Flatten all config file paths
    services = deployment_config.get("services", [])
    config_files = []
    for service in services:
        cfgs = service.get("config_paths")
        if not cfgs:
            print(
                f"⚠️ No config paths defined for service: {service.get('name', 'unknown')}"
            )
            continue
        config_files.extend(cfgs)

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

        print(f"Injecting secrets into {full_path}")
        try:
            config_data = load_json(full_path)
            merged = {**config_data, **secrets}
            save_json(merged, full_path)
            print(f"✅ Injected secrets into {full_path}")
        except Exception as e:
            print(f"❌ Error processing {full_path}: {e}")
            sys.exit(1)

    print("✅ All configs updated successfully.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Inject secrets into JSON config files based on deployment config."
    )
    parser.add_argument(
        "--deployment_config_path",
        type=Path,
        required=True,
        help="Path to the deployment config JSON file",
    )
    parser.add_argument(
        "--secrets_json",
        required=True,
        help="Secrets as a JSON string or environment variable name prefixed with ENV:",
    )

    args = parser.parse_args()

    main(args.deployment_config_path, args.secrets_json)
