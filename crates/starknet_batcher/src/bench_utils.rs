use blockifier::test_utils::transfers_generator::{
    TransfersGenerator as BlockifierTransactionGenerator,
    TransfersGeneratorConfig,
};

pub struct TransfersGenerator(pub BlockifierTransactionGenerator);

impl TransfersGenerator {
    pub fn new(config: TransfersGeneratorConfig) -> Self {
        Self(BlockifierTransactionGenerator::new(config))
    }

    pub fn execute_transfers(&mut self) {
        self.0.execute_transfers();
    }
}
