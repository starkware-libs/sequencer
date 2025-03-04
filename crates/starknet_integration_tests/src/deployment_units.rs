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
