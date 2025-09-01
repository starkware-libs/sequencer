#!/usr/bin/env python3
"""
Test script to verify YAML formatting fix
"""

import json
import yaml


def represent_literal_str(dumper, data):
    """Custom representer for literal scalar strings"""
    if "\n" in data:
        return dumper.represent_scalar("tag:yaml.org,2002:str", data, style="|")
    return dumper.represent_scalar("tag:yaml.org,2002:str", data)


def test_yaml_formatting():
    # Sample input similar to what the script processes
    input_yaml = """apiVersion: v1
data:
  config: |-
    {
      "base_layer_config.node_url": "http://localhost:53260/",
      "base_layer_config.prague_blob_gas_calc": true,
      "base_layer_config.timeout_millis": 1000,
      "consensus_manager_config.immediate_active_height": 100,
      "validator_id": "0x40"
    }
kind: ConfigMap
metadata:
  name: sequencer-core-config
"""

    print("Original YAML:")
    print(input_yaml)
    print("\n" + "=" * 80 + "\n")

    # Parse the YAML
    config = yaml.safe_load(input_yaml)

    # Parse and modify the JSON config
    config_str = config["data"]["config"].strip()
    config_data = json.loads(config_str)

    # Make some changes (similar to the actual script)
    config_data["consensus_manager_config.immediate_active_height"] = 150
    config_data["validator_id"] = "0x41"

    # Put the updated config back
    config["data"]["config"] = json.dumps(config_data, indent=2)

    print("Without fix (default YAML dump):")
    default_output = yaml.dump(config, default_flow_style=False)
    print(default_output)
    print("\n" + "=" * 80 + "\n")

    print("With fix (literal scalar style):")
    # Configure YAML dumper to use literal style for multi-line strings
    yaml.add_representer(str, represent_literal_str)

    try:
        fixed_output = yaml.dump(config, default_flow_style=False, allow_unicode=True)
        print(fixed_output)
    finally:
        # Clean up the custom representer
        yaml.add_representer(str, yaml.representer.SafeRepresenter.represent_str)


if __name__ == "__main__":
    test_yaml_formatting()
