use std::fmt::Debug;

use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::task;

#[derive(Debug, Copy, Clone)]
struct InboundA {
    pub value: i32,
}

#[derive(Debug, Copy, Clone)]
struct InboundB {
    pub value: i32,
}

#[derive(Debug, Copy, Clone)]
struct InboundC {
    pub value: i32,
}

impl From<InboundA> for InboundB {
    fn from(a: InboundA) -> Self {
        Self { value: a.value }
    }
}

impl From<InboundA> for InboundC {
    fn from(a: InboundA) -> Self {
        Self { value: a.value }
    }
}

impl From<InboundB> for InboundC {
    fn from(b: InboundB) -> Self {
        Self { value: b.value }
    }
}

impl From<InboundB> for InboundA {
    fn from(b: InboundB) -> Self {
        Self { value: b.value }
    }
}

impl From<InboundC> for InboundA {
    fn from(c: InboundC) -> Self {
        Self { value: c.value }
    }
}

impl From<InboundC> for InboundB {
    fn from(c: InboundC) -> Self {
        Self { value: c.value }
    }
}

struct TestComponentA {
    pub inbound: Receiver<InboundA>,
    pub outbound_b: Sender<InboundB>,
    pub outbound_c: Sender<InboundC>,
}
struct TestComponentB {
    pub inbound: Receiver<InboundB>,
}

struct TestComponentC {
    pub inbound: Receiver<InboundC>,
}

impl TestComponentA {
    pub async fn start(&mut self) {
        loop {
            let val = self.inbound.recv().await.unwrap();
            println!("A Received: {:?}", val);
            self.outbound_b.send(val.into()).await.unwrap();
            self.outbound_c.send(val.into()).await.unwrap();
        }
    }
}

impl TestComponentB {
    pub async fn start(&mut self) {
        loop {
            let val = self.inbound.recv().await.unwrap();

            println!("B Received: {:?}", val);
        }
    }
}

impl TestComponentC {
    pub async fn start(&mut self) {
        loop {
            let val = self.inbound.recv().await.unwrap();
            println!("C Received: {:?}", val);
        }
    }
}

#[tokio::test]
async fn test_send_and_receive() {
    let (tx_a, rx_a) = channel::<InboundA>(5);
    let (tx_b, rx_b) = channel::<InboundB>(5);
    let (tx_c, rx_c) = channel::<InboundC>(5);

    let mut a =
        TestComponentA { inbound: rx_a, outbound_b: tx_b.clone(), outbound_c: tx_c.clone() };
    let mut b = TestComponentB { inbound: rx_b };
    let mut c = TestComponentC { inbound: rx_c };

    task::spawn(async move {
        a.start().await;
    });

    task::spawn(async move {
        b.start().await;
    });

    task::spawn(async move {
        c.start().await;
    });

    tx_a.send(InboundA { value: 1 }).await.unwrap();
}
