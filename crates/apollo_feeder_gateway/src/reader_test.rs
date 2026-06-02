use crate::reader::{ChainDataReader, MockChainDataReader};

#[tokio::test]
async fn mock_chain_data_reader_returns_configured_value() {
    let mut mock = MockChainDataReader::new();
    mock.expect_latest_block_header().returning(|| Ok(None));

    let header = mock.latest_block_header().await.unwrap();
    assert!(header.is_none());
}
