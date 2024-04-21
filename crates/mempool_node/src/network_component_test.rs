mod tests {
    use crate::network_component::NetworkComponent;
    use tokio::{sync::mpsc::channel, task};

    #[tokio::test]
    async fn test_send_and_receive() {
        type AtoB = u32;
        type BtoA = i32;

        struct A {
            pub network: NetworkComponent<AtoB, BtoA>,
        }
        struct B {
            pub network: NetworkComponent<BtoA, AtoB>,
        }

        let (tx_a2b, rx_a2b) = channel::<AtoB>(1);
        let (tx_b2a, rx_b2a) = channel::<BtoA>(1);

        let network_a = NetworkComponent::new(tx_a2b, rx_b2a);
        let network_b = NetworkComponent::new(tx_b2a, rx_a2b);

        let a = A { network: network_a };
        let mut b = B { network: network_b };

        task::spawn(async move {
            let a2b: AtoB = 1;
            a.network.tx.send(a2b).await.unwrap();
        })
        .await
        .unwrap();

        let ret = task::spawn(async move { b.network.rx.recv().await.unwrap() })
            .await
            .unwrap();

        let expected_ret: AtoB = 1;
        assert_eq!(ret, expected_ret);
    }
}
