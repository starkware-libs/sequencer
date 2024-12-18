#[derive(Debug)]
pub enum TestIdentifier {
    EndToEndIntegrationTest,
    EndToEndFlowTest,
}

impl From<TestIdentifier> for u16 {
    fn from(variant: TestIdentifier) -> Self {
        match variant {
            TestIdentifier::EndToEndIntegrationTest => 0,
            TestIdentifier::EndToEndFlowTest => 1,
        }
    }
}
