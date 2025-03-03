use crate::consts::{
    BATCHER_DEPLOYMENT_UNIT_CONFIG_PATH,
    CLASS_MANAGER_DEPLOYMENT_UNIT_CONFIG_PATH,
    GATEWAY_DEPLOYMENT_UNIT_CONFIG_PATH,
    HTTP_SERVER_DEPLOYMENT_UNIT_CONFIG_PATH,
    L1_PROVIDER_DEPLOYMENT_UNIT_CONFIG_PATH,
    MEMPOOL_DEPLOYMENT_UNIT_CONFIG_PATH,
    SIERRA_COMPILER_DEPLOYMENT_UNIT_CONFIG_PATH,
    STATE_SYNC_DEPLOYMENT_UNIT_CONFIG_PATH,
};

// TODO(alonl): consider renaming to AtomicDeploymentUnit
pub enum DeploymentUnitType {
    Batcher,
    ClassManager,
    Gateway,
    Mempool,
    SierraCompiler,
    StateSync,
    ConsensusManager,
    HttpServer,
    L1Provider,
}

pub struct DeploymentUnit {
    pub config_path: String,
    pub machine_type: String,
    pub storage: bool,
    pub replicas: usize,
}

pub fn get_deployment_unit(deployment_unit_type: DeploymentUnitType) -> DeploymentUnit {
    match deployment_unit_type {
        DeploymentUnitType::Batcher => DeploymentUnit {
            config_path: BATCHER_DEPLOYMENT_UNIT_CONFIG_PATH.to_string(),
            machine_type: "".to_string(),
            storage: true,
            replicas: 1,
        },
        DeploymentUnitType::ClassManager => DeploymentUnit {
            config_path: CLASS_MANAGER_DEPLOYMENT_UNIT_CONFIG_PATH.to_string(),
            machine_type: "".to_string(),
            storage: true,
            replicas: 1,
        },
        DeploymentUnitType::Gateway => DeploymentUnit {
            config_path: GATEWAY_DEPLOYMENT_UNIT_CONFIG_PATH.to_string(),
            machine_type: "".to_string(),
            storage: false,
            replicas: 1,
        },
        DeploymentUnitType::Mempool => DeploymentUnit {
            config_path: MEMPOOL_DEPLOYMENT_UNIT_CONFIG_PATH.to_string(),
            machine_type: "".to_string(),
            storage: true,
            replicas: 1,
        },
        DeploymentUnitType::SierraCompiler => DeploymentUnit {
            config_path: SIERRA_COMPILER_DEPLOYMENT_UNIT_CONFIG_PATH.to_string(),
            machine_type: "".to_string(),
            storage: false,
            replicas: 1,
        },
        DeploymentUnitType::StateSync => DeploymentUnit {
            config_path: STATE_SYNC_DEPLOYMENT_UNIT_CONFIG_PATH.to_string(),
            machine_type: "".to_string(),
            storage: true,
            replicas: 1,
        },
        DeploymentUnitType::ConsensusManager => DeploymentUnit {
            config_path: CLASS_MANAGER_DEPLOYMENT_UNIT_CONFIG_PATH.to_string(),
            machine_type: "".to_string(),
            storage: false,
            replicas: 1,
        },
        DeploymentUnitType::HttpServer => DeploymentUnit {
            config_path: HTTP_SERVER_DEPLOYMENT_UNIT_CONFIG_PATH.to_string(),
            machine_type: "".to_string(),
            storage: false,
            replicas: 1,
        },
        DeploymentUnitType::L1Provider => DeploymentUnit {
            config_path: L1_PROVIDER_DEPLOYMENT_UNIT_CONFIG_PATH.to_string(),
            machine_type: "".to_string(),
            storage: false,
            replicas: 1,
        },
    }
}
