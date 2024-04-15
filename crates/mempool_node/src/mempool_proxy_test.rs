mod tests {

    use std::sync::Arc;

    use tokio::task::JoinSet;

    use crate::{
        mempool::{AddTransactionCallType, AddTransactionReturnType, Mempool, MempoolTrait},
        mempool_proxy::MempoolProxy,
    };

    async fn test_mempool_single_thread_add_transaction<T>(mempool: T)
    where
        T: MempoolTrait,
    {
        let tx: AddTransactionCallType = 1;
        let expected_result: AddTransactionReturnType = 1;
        assert_eq!(mempool.add_transaction(tx).await, expected_result);
    }

    async fn test_mempool_concurrent_add_transaction<T>(mempool: Arc<T>)
    where
        T: MempoolTrait + std::marker::Send + std::marker::Sync + 'static,
    {
        let mut tasks: JoinSet<_> = (0..5)
            .map(|_| {
                let mempool = mempool.clone();
                async move {
                    let tx: AddTransactionCallType = 1;
                    mempool.add_transaction(tx).await
                }
            })
            .collect();

        let mut results: Vec<AddTransactionReturnType> = vec![];
        while let Some(result) = tasks.join_next().await {
            results.push(result.unwrap());
        }

        results.sort();

        let expected_results: Vec<AddTransactionReturnType> = (1..=5).collect();
        assert_eq!(results, expected_results);
    }

    #[tokio::test]
    async fn test_direct_mempool_single_thread_add_transaction() {
        test_mempool_single_thread_add_transaction(Mempool::default()).await;
    }

    #[tokio::test]
    async fn test_proxy_mempool_single_thread_add_transaction() {
        test_mempool_single_thread_add_transaction(MempoolProxy::default()).await;
    }

    #[tokio::test]
    async fn test_direct_mempool_concurrent_add_transaction() {
        test_mempool_concurrent_add_transaction(Arc::new(Mempool::default())).await;
    }

    #[tokio::test]
    async fn test_proxy_mempool_concurrent_add_transaction() {
        test_mempool_concurrent_add_transaction(Arc::new(MempoolProxy::default())).await;
    }
}
