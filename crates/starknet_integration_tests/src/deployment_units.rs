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
