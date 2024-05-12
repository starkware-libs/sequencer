pub mod component_runner;
pub mod network_component;

pub mod component_client;
pub mod component_server;

#[cfg(test)]
mod network_component_test;

#[cfg(test)]
mod channels_test;

#[cfg(test)]
mod component_runner_test;

#[cfg(test)]
mod component_server_client_test;
