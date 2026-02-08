# TODO(Tsabary): remove this entire module.

from enum import Enum


class EnvironmentName(Enum):
    DEV = "dev"
    INTEGRATION = "integration"
    TESTNET = "testnet"
    MAINNET = "mainnet"


# Translates the environment name to a suffix for alert filenames. We use the `mainnet` setting for development and the mainnet environment.
# The `testnet` setting is used for integration and testnet environments.
def alert_env_filename_suffix(env: EnvironmentName) -> str:
    env_to_alert_filename_suffix_mapping = {
        EnvironmentName.DEV: "mainnet",
        EnvironmentName.INTEGRATION: "testnet",
        EnvironmentName.TESTNET: "testnet",
        EnvironmentName.MAINNET: "mainnet",
    }
    return env_to_alert_filename_suffix_mapping[env]
