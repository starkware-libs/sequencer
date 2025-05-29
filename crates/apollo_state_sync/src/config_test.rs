use apollo_network::NetworkConfig;
use apollo_p2p_sync::server::P2pSyncServerConfig;
use assert_matches::assert_matches;
use validator::{Validate, ValidationErrors};

use crate::config::StateSyncConfig;

fn get_valid_state_sync_config() -> StateSyncConfig {
    StateSyncConfig {
        storage_config: Default::default(),
        p2p_sync_client_config: Some(Default::default()),
        central_sync_client_config: None,
        p2p_sync_server_config: None,
        network_config: None,
        revert_config: Default::default(),
        rpc_config: Default::default(),
    }
}

// This test is used to make sure we can rely on get_valid_state_sync_config in the tests below.
#[test]
fn valid_config_returns_valud_config() {
    assert_matches!(get_valid_state_sync_config().validate(), Ok(_));
}

#[test]
fn validate_config_invalid_for_central_and_p2p_sync_client() {
    let valid_config = get_valid_state_sync_config();
    assert_matches!(
        StateSyncConfig {
            p2p_sync_client_config: Some(Default::default()),
            central_sync_client_config: Some(Default::default()),
            ..valid_config
        }
        .validate(),
        Err(ValidationErrors { .. })
    );
}

#[test]
fn validate_config_invalid_for_no_central_and_no_p2p_sync_client() {
    let valid_config = get_valid_state_sync_config();
    assert_matches!(
        StateSyncConfig {
            p2p_sync_client_config: None,
            central_sync_client_config: None,
            ..valid_config
        }
        .validate(),
        Err(ValidationErrors { .. })
    );
}

#[test]
fn validate_config_valid_for_central_and_no_p2p_sync_client() {
    let valid_config = get_valid_state_sync_config();
    assert_matches!(
        StateSyncConfig {
            p2p_sync_client_config: None,
            central_sync_client_config: Some(Default::default()),
            ..valid_config
        }
        .validate(),
        Ok(_)
    );
}

#[test]
fn validate_config_valid_for_no_central_and_p2p_sync_client() {
    let valid_config = get_valid_state_sync_config();
    assert_matches!(
        StateSyncConfig {
            p2p_sync_client_config: Some(Default::default()),
            central_sync_client_config: None,
            ..valid_config
        }
        .validate(),
        Ok(_)
    );
}

#[test]
fn validate_config_valid_for_no_network_and_no_p2p_sync_server() {
    let valid_config = get_valid_state_sync_config();
    assert_matches!(
        StateSyncConfig { p2p_sync_server_config: None, network_config: None, ..valid_config }
            .validate(),
        Ok(_)
    );
}

#[test]
fn validate_config_valid_for_both_network_and_p2p_sync_server() {
    let valid_config = get_valid_state_sync_config();
    assert_matches!(
        StateSyncConfig {
            p2p_sync_server_config: Some(P2pSyncServerConfig::default()),
            network_config: Some(NetworkConfig::default()),
            ..valid_config
        }
        .validate(),
        Ok(_)
    );
}

#[test]
fn validate_config_invalid_for_network_and_no_p2p_sync_server() {
    let valid_config = get_valid_state_sync_config();
    assert_matches!(
        StateSyncConfig {
            p2p_sync_server_config: None,
            network_config: Some(NetworkConfig::default()),
            ..valid_config
        }
        .validate(),
        Err(ValidationErrors { .. })
    );
}

#[test]
fn validate_config_invalid_for_no_network_and_p2p_sync_server() {
    let valid_config = get_valid_state_sync_config();
    assert_matches!(
        StateSyncConfig {
            p2p_sync_server_config: Some(P2pSyncServerConfig::default()),
            network_config: None,
            ..valid_config
        }
        .validate(),
        Err(ValidationErrors { .. })
    );
}
