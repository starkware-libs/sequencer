use std::collections::BTreeMap;

use assert_matches::assert_matches;
use async_trait::async_trait;
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use pretty_assertions::assert_eq;
use serde::{Deserialize, Serialize};

use crate::component_runner::{ComponentRunner, ComponentStartError};

mod test_component_a {
    use super::*;

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct TestConfigA {
        pub bool_field: bool,
    }

    impl SerializeConfig for TestConfigA {
        fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
            BTreeMap::from_iter([ser_param(
                "test1",
                &self.bool_field,
                "...",
                ParamPrivacyInput::Public,
            )])
        }
    }

    #[derive(Debug)]
    pub struct TestComponentA {
        pub config: TestConfigA,
    }

    impl TestComponentA {
        async fn local_start(&self) -> Result<(), tokio::io::Error> {
            println!("TestComponent1::local_start(), config: {:#?}", self.config);
            Ok(())
        }
    }

    #[async_trait]
    impl ComponentRunner for TestComponentA {
        async fn start(&mut self) -> Result<(), ComponentStartError> {
            println!("TestComponent1::start(), component: {:#?}", self);
            self.local_start().await.map_err(|_err| ComponentStartError::InternalComponentError)
        }
    }
}

mod test_component_b {
    use super::*;

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
    pub struct TestConfigB {
        pub u32_field: u32,
    }

    impl SerializeConfig for TestConfigB {
        fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
            BTreeMap::from_iter([ser_param(
                "test2",
                &self.u32_field,
                "...",
                ParamPrivacyInput::Public,
            )])
        }
    }

    #[derive(Debug)]
    pub struct TestComponentB {
        pub config: TestConfigB,
    }

    #[async_trait]
    impl ComponentRunner for TestComponentB {
        async fn start(&mut self) -> Result<(), ComponentStartError> {
            println!("TestComponent2::start(): component: {:#?}", self);
            match self.config.u32_field {
                43 => Err(ComponentStartError::InternalComponentError),
                44 => Err(ComponentStartError::ComponentConfigError),
                _ => Ok(()),
            }
        }
    }
}

use test_component_a::{TestComponentA, TestConfigA};

#[tokio::test]
async fn test_component_a() {
    let test_config = TestConfigA { bool_field: true };
    let mut component = TestComponentA { config: test_config };
    assert_matches!(component.start().await, Ok(()));
}

use test_component_b::{TestComponentB, TestConfigB};

#[tokio::test]
async fn test_component_b() {
    let test_config = TestConfigB { u32_field: 42 };
    let mut component = TestComponentB { config: test_config };
    assert_matches!(component.start().await, Ok(()));

    let test_config = TestConfigB { u32_field: 43 };
    let mut component = TestComponentB { config: test_config };
    assert_matches!(component.start().await, Err(e) => {
        assert_eq!(e, ComponentStartError::InternalComponentError);
    });

    let test_config = TestConfigB { u32_field: 44 };
    let mut component = TestComponentB { config: test_config };
    assert_matches!(component.start().await, Err(e) => {
        assert_eq!(e, ComponentStartError::ComponentConfigError);
    });
}
