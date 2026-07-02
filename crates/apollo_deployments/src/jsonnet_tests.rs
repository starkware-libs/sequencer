//! Parity test helpers driven off the jsonnet evaluator (`jsonnet_eval`): assert that every
//! service's `build()` output deserializes into `SequencerNodeConfig`. Compiled only under `test`;
//! the reusable evaluator core lives in `jsonnet_eval`.

use apollo_node_config::node_config::SequencerNodeConfig;
use serde_json::Value;
use strum::IntoEnumIterator;

use crate::jsonnet_eval::build_service_configs;
use crate::service::{NodeService, NodeType};

const TESTING_CHAIN_PARAMS_PATH: &str = "testing/chain_params.libsonnet";
const TESTING_NODE_PARAMS_PATH: &str = "testing/node_params.libsonnet";

/// Asserts that `build(layout, params)` produces, for every service of layout `S`, an object that
/// deserializes into `SequencerNodeConfig`.
pub(crate) fn assert_build_deserializes<S>()
where
    S: IntoEnumIterator + Into<NodeService>,
{
    let some_service: NodeService =
        S::iter().next().expect("a layout has at least one service").into();
    let layout = NodeType::from(&some_service).to_string();
    let built = eval_build(&layout);
    let services = built.as_object().unwrap();

    // Sanity check: the build result should have at least one service.
    assert!(!services.is_empty(), "build({layout}) produced no services");

    for (service_name, config) in services {
        let mut node_config = serde_json::from_value::<SequencerNodeConfig>(config.clone())
            .unwrap_or_else(|error| {
                panic!(
                    "service {service_name} of layout {layout} does not deserialize into \
                     SequencerNodeConfig: {error}"
                )
            });
        // Component urls are the in-cluster service DNS names baked in by the jsonnet build (e.g.
        // `sequencer-core-service`), which don't resolve off-cluster; `validate_node_config`
        // resolves every component url. Rewrite them to localhost — the same helper the
        // integration-test config builders use — so validation reaches the cross-component
        // invariants. url/port are deploy-time placeholders anyway.
        node_config.components.set_urls_to_localhost();
        // The build output must also satisfy the cross-component invariants (chain_id and the other
        // formerly-pointer-resolved values agreeing across components, etc.). Without this, a
        // jsonnet change that broke a pointer group would pass CI and only fail at prod boot.
        node_config.validate_node_config().unwrap_or_else(|error| {
            panic!(
                "service {service_name} of layout {layout} deserializes but fails \
                 validate_node_config: {error}"
            )
        });
    }
}

/// Evaluates `build(layout, <testing params>)` and returns its JSON as a `Value`: a map from
/// service name to that service's fully-assembled config. The testing params supply only the
/// mandatory chain_params + node_params buckets; `replacers` is omitted, so every replacer falls
/// back to its applicative-config default.
fn eval_build(layout: &str) -> Value {
    let params = format!(
        "{{ chain_params: import '{TESTING_CHAIN_PARAMS_PATH}', node_params: import \
         '{TESTING_NODE_PARAMS_PATH}' }}"
    );
    Value::Object(build_service_configs(layout, &params))
}
