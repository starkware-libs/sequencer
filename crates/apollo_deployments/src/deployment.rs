use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use apollo_config::dumping::{prepend_sub_config_name, SerializeConfig};
use apollo_config::{ParamPath, SerializedParam};
use apollo_infra_utils::dumping::serialize_to_file;
#[cfg(test)]
use apollo_infra_utils::dumping::serialize_to_file_test;
use apollo_node::config::component_config::ComponentConfig;
use apollo_node::config::config_utils::config_to_preset;
use apollo_protobuf::consensus::DEFAULT_VALIDATOR_ID;
use indexmap::IndexMap;
use serde::Serialize;
use serde_json::{json, to_value, Value};
use starknet_api::core::ChainId;

use crate::deployment_definitions::{Environment, CONFIG_BASE_DIR};
use crate::service::{DeploymentName, ExternalSecret, IngressParams, Service, ServiceName};

#[cfg(test)]
pub(crate) const FIX_BINARY_NAME: &str = "deployment_generator";

const DEPLOYMENT_CONFIG_DIR_NAME: &str = "deployment_configs/";

const DEPLOYMENT_FILE_NAME: &str = "deployment_config_override.json";
const INSTANCE_FILE_NAME: &str = "instance_config_override.json";

const MAX_NODE_ID: usize = 9; // Currently supporting up to 9 nodes, to avoid more complicated string manipulations.

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Deployment {
    chain_id: ChainId,
    application_config_subdir: PathBuf,
    #[serde(skip_serializing)]
    deployment_name: DeploymentName,
    #[serde(skip_serializing)]
    environment: Environment,
    services: Vec<Service>,
    #[serde(skip_serializing)]
    instance_name: String,
    #[serde(skip_serializing)]
    base_app_config_file_path: PathBuf,
}

// TODO(Tsabary): reduce number of args.
#[allow(clippy::too_many_arguments)]
impl Deployment {
    pub fn new(
        chain_id: ChainId,
        deployment_name: DeploymentName,
        environment: Environment,
        instance_name: &str,
        external_secret: Option<ExternalSecret>,
        base_app_config_file_path: PathBuf,
        config_override: ConfigOverride,
        ingress_params: IngressParams,
    ) -> Self {
        let service_names = deployment_name.all_service_names();

        let application_config_subdir = deployment_name
            .add_path_suffix(environment.application_config_dir_path(), instance_name);

        // TODO(Tsabary): list the mutual parent dir of the base app config and all the services'
        // configs as the parent dir, and for each file add its specific path originating from that
        // dir. This will enable removing the current "upward" paths.

        // Reference the base app config file from the application config subdir.
        let base_app_config_relative_path =
            relative_up_path(&application_config_subdir, &base_app_config_file_path);

        let config_override_files: Vec<String> = config_override.create(&application_config_subdir);

        let additional_config_filenames: Vec<String> =
            std::iter::once(base_app_config_relative_path.to_string_lossy().to_string())
                .chain(config_override_files)
                .collect();

        let services = service_names
            .iter()
            .map(|service_name| {
                service_name.create_service(
                    &environment,
                    &external_secret,
                    additional_config_filenames.clone(),
                    ingress_params.clone(),
                )
            })
            .collect();
        Self {
            chain_id,
            application_config_subdir,
            deployment_name,
            environment,
            services,
            instance_name: instance_name.to_string(),
            base_app_config_file_path,
        }
    }

    pub fn get_deployment_name(&self) -> &DeploymentName {
        &self.deployment_name
    }

    pub fn get_base_app_config_file_path(&self) -> PathBuf {
        self.base_app_config_file_path.clone()
    }

    pub fn application_config_values(&self) -> IndexMap<ServiceName, Value> {
        let component_configs = self.deployment_name.get_component_configs(None, &self.environment);
        let mut result = IndexMap::new();

        let l1_provider_config = self.environment.get_l1_provider_config_modifications().as_value();

        for (service, component_config) in component_configs.into_iter() {
            // Component configs, determined by the service.

            let component_config_serialization_wrapper: ComponentConfigsSerializationWrapper =
                component_config.into();

            let mut flattened_component_config_map =
                config_to_preset(&json!(component_config_serialization_wrapper.dump()));

            // Unify maps of component configs and L1 provider configs.
            // TODO(Tsabary): l1 provider config should be dumped in a different file
            if let (Value::Object(obj1), Value::Object(obj2)) =
                (&mut flattened_component_config_map, l1_provider_config.clone())
            {
                obj1.extend(obj2);
            }

            result.insert(service, flattened_component_config_map);
        }

        result
    }

    pub fn dump_application_config_files(&self) {
        let app_configs = self.application_config_values();
        for (service, value) in app_configs.into_iter() {
            let config_path = &self.application_config_subdir.join(service.get_config_file_path());
            serialize_to_file(
                value,
                config_path.to_str().expect("Should be able to convert path to string"),
            );
        }
    }

    pub fn get_config_file_paths(&self) -> Vec<Vec<String>> {
        self.services
            .iter()
            .map(|service| {
                service
                    .get_config_paths()
                    .into_iter()
                    .map(|s| format!("{}{}", self.application_config_subdir.to_string_lossy(), s))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    pub fn deployment_file_path(&self) -> PathBuf {
        PathBuf::from(CONFIG_BASE_DIR)
            .join(self.environment.to_string())
            .join(DEPLOYMENT_CONFIG_DIR_NAME)
            .join(format!("{}.json", self.instance_name))
    }

    #[cfg(test)]
    pub(crate) fn assert_application_configs_exist(&self) {
        for service in &self.services {
            for config_path in service.get_config_paths() {
                // Concatenate paths.
                let full_path = &self.application_config_subdir.join(config_path);
                // Assert existence.
                assert!(full_path.exists(), "File does not exist: {:?}", full_path);
            }
        }
    }

    #[cfg(test)]
    pub fn test_dump_application_config_files(&self) {
        let app_configs = self.application_config_values();
        for (service, value) in app_configs.into_iter() {
            let config_path = &self.application_config_subdir.join(service.get_config_file_path());
            serialize_to_file_test(
                value,
                config_path.to_str().expect("Should be able to convert path to string"),
                FIX_BINARY_NAME,
            );
        }
    }
}

// TODO(Tsabary): test no conflicts between config entries defined in each of the override types.
// TODO(Tsabary): delete duplicates from the base app config, and add a test that there are no
// conflicts between all the override config entries and the values in the base app config.

pub(crate) fn format_node_id(base_format: &str, id: usize) -> String {
    base_format.replace("{}", &id.to_string())
}

#[derive(Debug, Serialize)]
pub struct ConfigOverride {
    deployment_config_override: DeploymentConfigOverride,
    instance_config_override: InstanceConfigOverride,
}

impl ConfigOverride {
    pub const fn new(
        deployment_config_override: DeploymentConfigOverride,
        instance_config_override: InstanceConfigOverride,
    ) -> Self {
        Self { deployment_config_override, instance_config_override }
    }

    pub fn create(&self, application_config_subdir: &Path) -> Vec<String> {
        serialize_to_file(
            to_value(&self.deployment_config_override).unwrap(),
            application_config_subdir.join(DEPLOYMENT_FILE_NAME).to_str().unwrap(),
        );

        serialize_to_file(
            to_value(&self.instance_config_override).unwrap(),
            application_config_subdir.join(INSTANCE_FILE_NAME).to_str().unwrap(),
        );
        vec![DEPLOYMENT_FILE_NAME.to_string(), INSTANCE_FILE_NAME.to_string()]
    }
}

#[derive(Debug, Serialize)]
pub struct DeploymentConfigOverride {
    #[serde(rename = "base_layer_config.starknet_contract_address")]
    starknet_contract_address: String,
    chain_id: String,
    eth_fee_token_address: String,
    starknet_url: String,
    strk_fee_token_address: String,
}

impl DeploymentConfigOverride {
    pub fn new(
        starknet_contract_address: impl ToString,
        chain_id: impl ToString,
        eth_fee_token_address: impl ToString,
        starknet_url: impl ToString,
        strk_fee_token_address: impl ToString,
    ) -> Self {
        Self {
            starknet_contract_address: starknet_contract_address.to_string(),
            chain_id: chain_id.to_string(),
            eth_fee_token_address: eth_fee_token_address.to_string(),
            starknet_url: starknet_url.to_string(),
            strk_fee_token_address: strk_fee_token_address.to_string(),
        }
    }
}

// TODO(Tsabary): re-verify all config diffs.

#[derive(Debug, Serialize)]
pub struct InstanceConfigOverride {
    #[serde(rename = "consensus_manager_config.network_config.bootstrap_peer_multiaddr")]
    consensus_bootstrap_peer_multiaddr: String,
    #[serde(rename = "consensus_manager_config.network_config.bootstrap_peer_multiaddr.#is_none")]
    consensus_bootstrap_peer_multiaddr_is_none: bool,
    // TODO(Tsabary): network secret keys should be defined as secrets.
    #[serde(rename = "consensus_manager_config.network_config.secret_key")]
    consensus_secret_key: String,
    #[serde(rename = "mempool_p2p_config.network_config.bootstrap_peer_multiaddr")]
    mempool_bootstrap_peer_multiaddr: String,
    #[serde(rename = "mempool_p2p_config.network_config.bootstrap_peer_multiaddr.#is_none")]
    mempool_bootstrap_peer_multiaddr_is_none: bool,
    // TODO(Tsabary): network secret keys should be defined as secrets.
    #[serde(rename = "mempool_p2p_config.network_config.secret_key")]
    mempool_secret_key: String,
    validator_id: String,
}

impl InstanceConfigOverride {
    pub fn new(
        consensus_bootstrap_peer_multiaddr: impl ToString,
        consensus_bootstrap_peer_multiaddr_is_none: bool,
        consensus_secret_key: impl ToString,
        mempool_bootstrap_peer_multiaddr: impl ToString,
        mempool_bootstrap_peer_multiaddr_is_none: bool,
        mempool_secret_key: impl ToString,
        validator_id: impl ToString,
    ) -> Self {
        Self {
            consensus_bootstrap_peer_multiaddr: consensus_bootstrap_peer_multiaddr.to_string(),
            consensus_bootstrap_peer_multiaddr_is_none,
            consensus_secret_key: consensus_secret_key.to_string(),
            mempool_bootstrap_peer_multiaddr: mempool_bootstrap_peer_multiaddr.to_string(),
            mempool_bootstrap_peer_multiaddr_is_none,
            mempool_secret_key: mempool_secret_key.to_string(),
            validator_id: validator_id.to_string(),
        }
    }
}

pub(crate) enum DeploymentType {
    Bootstrap,
    Operational,
}

impl DeploymentType {
    fn validator_id_offset(&self) -> usize {
        match self {
            DeploymentType::Bootstrap => 1,
            DeploymentType::Operational => DEFAULT_VALIDATOR_ID.try_into().unwrap(),
        }
    }
}

pub(crate) fn create_hybrid_instance_config_override(
    id: usize,
    namespace: &'static str,
    deployment_type: DeploymentType,
) -> InstanceConfigOverride {
    assert!(id < MAX_NODE_ID, "Node id {} exceeds the number of nodes {}", id, MAX_NODE_ID);

    // TODO(Tsabary): these should be derived from the hybrid deployment module, and used
    // consistently throughout the code.

    // This node address uses that the first node secret key is
    // "0x0101010101010101010101010101010101010101010101010101010101010101".
    // TODO(Tsabary): test to enforce the above assumption.
    const FIRST_NODE_ADDRESS: &str = "12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5";
    const CORE_SERVICE_NAME: &str = "sequencer-core-service";
    const CORE_SERVICE_PORT: u16 = 53080;

    const MEMPOOL_SERVICE_NAME: &str = "sequencer-mempool-service";
    const MEMPOOL_SERVICE_PORT: u16 = 53200;

    if id == 0 {
        InstanceConfigOverride::new(
            "",
            true,
            get_secret_key(id),
            "",
            true,
            get_secret_key(id),
            get_validator_id(id, deployment_type),
        )
    } else {
        InstanceConfigOverride::new(
            format!(
                "/dns/{}.{}.svc.cluster.local/tcp/{}/p2p/{}",
                CORE_SERVICE_NAME, namespace, CORE_SERVICE_PORT, FIRST_NODE_ADDRESS
            ),
            false,
            get_secret_key(id),
            format!(
                "/dns/{}.{}.svc.cluster.local/tcp/{}/p2p/{}",
                MEMPOOL_SERVICE_NAME, namespace, MEMPOOL_SERVICE_PORT, FIRST_NODE_ADDRESS
            ),
            false,
            get_secret_key(id),
            get_validator_id(id, deployment_type),
        )
    }
}

fn get_secret_key(id: usize) -> String {
    format!("0x010101010101010101010101010101010101010101010101010101010101010{}", id + 1)
}

// TODO(Tsabary): Return this back to observer mode and add a way to configure between observer and
// non observer. Also change the mempool ttl to 100000 and startup_rewind_time_seconds to 28800 in
// observer mode
fn get_validator_id(id: usize, deployment_type: DeploymentType) -> String {
    format!("0x{:x}", id + deployment_type.validator_id_offset())
}

fn relative_up_path(from: &Path, to: &Path) -> PathBuf {
    // Canonicalize logically (NOT on filesystem)
    let from_components: Vec<_> = from.components().collect();
    let to_components: Vec<_> = to.components().collect();

    // Find common prefix length
    let common_len = from_components.iter().zip(&to_components).take_while(|(a, b)| a == b).count();

    // How many directories to go up from `from` to get to common root
    let up_levels = from_components.len() - common_len;

    // Build the relative path
    let mut result = PathBuf::new();
    for _ in 0..up_levels {
        result.push("..");
    }
    for component in &to_components[common_len..] {
        result.push(component.as_os_str());
    }

    result
}

// A helper struct for serializing the components config in the same hierarchy as of its
// serialization as part of the entire config, i.e., by prepending "components.".
#[derive(Clone, Debug, Default, Serialize)]
struct ComponentConfigsSerializationWrapper {
    components: ComponentConfig,
}

impl From<ComponentConfig> for ComponentConfigsSerializationWrapper {
    fn from(value: ComponentConfig) -> Self {
        ComponentConfigsSerializationWrapper { components: value }
    }
}

impl SerializeConfig for ComponentConfigsSerializationWrapper {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        prepend_sub_config_name(self.components.dump(), "components")
    }
}
