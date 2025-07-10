use std::iter::once;
use std::path::PathBuf;

use serde::Serialize;

use crate::config_override::ConfigOverride;
use crate::deployment_definitions::{Environment, BASE_APP_CONFIG_PATH, CONFIG_BASE_DIR};
use crate::k8s::{ExternalSecret, IngressParams, K8SServiceType, K8sServiceConfigParams};
use crate::service::{NodeType, Service};

#[derive(Clone, Debug, Serialize)]
pub struct Deployment {
    application_config_subdir: PathBuf,
    services: Vec<Service>,
    #[serde(skip_serializing)]
    deployment_aux_data: DeploymentAuxData,
}

impl Deployment {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        node_type: NodeType,
        environment: Environment,
        instance_name: &str,
        external_secret: Option<ExternalSecret>,
        config_override: ConfigOverride,
        ingress_params: IngressParams,
        k8s_service_config_params: Option<K8sServiceConfigParams>,
    ) -> Self {
        let node_services = node_type.all_service_names();

        let config_override_files =
            config_override.get_config_file_paths(&environment.env_dir_path(), instance_name);
        let config_filenames: Vec<String> =
            once(BASE_APP_CONFIG_PATH.to_string()).chain(config_override_files).collect();

        let services = node_services
            .iter()
            .map(|node_service| {
                node_service.create_service(
                    &environment,
                    &external_secret,
                    config_filenames.clone(),
                    ingress_params.clone(),
                    k8s_service_config_params.clone(),
                )
            })
            .collect();
        Self {
            application_config_subdir: CONFIG_BASE_DIR.into(),
            services,
            deployment_aux_data: DeploymentAuxData {
                node_type,
                environment,
                instance_name: instance_name.to_string(),
                config_override,
            },
        }
    }

    pub fn get_node_type(&self) -> &NodeType {
        &self.deployment_aux_data.node_type
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
        self.deployment_aux_data
            .environment
            .env_dir_path()
            .join(format!("deployment_config_{}.json", self.deployment_aux_data.instance_name))
    }

    pub fn dump_config_override_files(&self) {
        self.deployment_aux_data.config_override.dump_config_files(
            &self.deployment_aux_data.environment.env_dir_path(),
            &self.deployment_aux_data.instance_name,
        );
    }

    #[cfg(test)]
    pub fn test_dump_config_override_files(&self) {
        self.deployment_aux_data.config_override.test_dump_config_files(
            &self.deployment_aux_data.environment.env_dir_path(),
            &self.deployment_aux_data.instance_name,
        );
    }
}

#[derive(Clone, Debug)]
struct DeploymentAuxData {
    node_type: NodeType,
    environment: Environment,
    instance_name: String,
    config_override: ConfigOverride,
}

<<<<<<< HEAD
// TODO(Tsabary): test no conflicts between config entries defined in each of the override types.
// TODO(Tsabary): delete duplicates from the base app config, and add a test that there are no
// conflicts between all the override config entries and the values in the base app config.

/// Represents the domain of the pragma directive in the configuration.
pub enum PragmaDomain {
    Dev,
    Prod,
}

impl Display for PragmaDomain {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let s = match self {
            PragmaDomain::Dev => "devnet",
            PragmaDomain::Prod => "production",
        };
        write!(f, "{s}")
    }
}

||||||| 7a2cf4d9a
// TODO(Tsabary): test no conflicts between config entries defined in each of the override types.
// TODO(Tsabary): delete duplicates from the base app config, and add a test that there are no
// conflicts between all the override config entries and the values in the base app config.

/// Represents the domain of the pragma directive in the configuration.
pub enum PragmaDomain {
    Dev,
    Prod,
}

impl Display for PragmaDomain {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let s = match self {
            PragmaDomain::Dev => "devnet",
            PragmaDomain::Prod => "production",
        };
        write!(f, "{}", s)
    }
}

=======
>>>>>>> origin/main-v0.14.0
// Creates the service name in the format: <node_service>.<namespace>.<domain>
pub(crate) fn build_service_namespace_domain_address(
    node_service: &str,
    namespace: &str,
    domain: &str,
) -> String {
    format!("{node_service}.{namespace}.{domain}")
}

// TODO(Tsabary): when transitioning runnings nodes in different clusters, this enum should be
// removed, and the p2p address should always be `External`.
#[derive(Clone)]
pub enum P2PCommunicationType {
    Internal,
    External,
}

impl P2PCommunicationType {
    pub(crate) fn get_p2p_domain(&self, domain: &str) -> String {
        match self {
            P2PCommunicationType::Internal => "svc.cluster.local",
            P2PCommunicationType::External => domain,
        }
        .to_string()
    }

    pub(crate) fn get_k8s_service_type(&self) -> K8SServiceType {
        K8SServiceType::LoadBalancer
    }
}
