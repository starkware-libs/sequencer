mod tests {
    use std::sync::Arc;

    use tokio::sync::Mutex;

    use crate::{
        mempool::{Mempool, MempoolTrait},
        mempool_proxy::MempoolProxy,
    };

    #[tokio::test]
    async fn test_proxy_add_transaction() {
        let mempool = Arc::new(Mutex::new(Mempool::new()));
        let mut proxy = MempoolProxy::new(mempool);
        assert_eq!(proxy.add_transaction(1).await, 1);
        assert_eq!(proxy.add_transaction(1).await, 2);
    }
}
