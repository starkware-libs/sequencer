#[cfg(test)]
#[path = "class_test.rs"]
mod class_test;

use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};

use papyrus_common::compression_utils::{compress_and_encode, decode_and_decompress};
use papyrus_common::pending_classes::ApiContractClass;
use papyrus_common::python_json::PythonJsonFormatter;
use prost::Message;
use serde::Serialize;
use starknet_api::contract_class::EntryPointType;
use starknet_api::core::{ClassHash, EntryPointSelector};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::{deprecated_contract_class, state};
use starknet_types_core::felt::Felt;

use super::common::volition_domain_to_enum_int;
use super::ProtobufConversionError;
use crate::sync::{ClassQuery, DataOrFin, Query};
use crate::{auto_impl_into_and_try_from_vec_u8, protobuf};

pub const DOMAIN: DataAvailabilityMode = DataAvailabilityMode::L1;

impl TryFrom<protobuf::ClassesResponse> for DataOrFin<(ApiContractClass, ClassHash)> {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::ClassesResponse) -> Result<Self, Self::Error> {
        match value.class_message {
            Some(protobuf::classes_response::ClassMessage::Class(class)) => {
                Ok(Self(Some(class.try_into()?)))
            }
            Some(protobuf::classes_response::ClassMessage::Fin(_)) => Ok(Self(None)),
            None => Err(ProtobufConversionError::MissingField {
                field_description: "ClassesResponse::class_message",
            }),
        }
    }
}
impl From<DataOrFin<(ApiContractClass, ClassHash)>> for protobuf::ClassesResponse {
    fn from(value: DataOrFin<(ApiContractClass, ClassHash)>) -> Self {
        match value.0 {
            Some(class) => protobuf::ClassesResponse {
                class_message: Some(protobuf::classes_response::ClassMessage::Class(class.into())),
            },
            None => protobuf::ClassesResponse {
                class_message: Some(protobuf::classes_response::ClassMessage::Fin(
                    protobuf::Fin {},
                )),
            },
        }
    }
}

auto_impl_into_and_try_from_vec_u8!(
    DataOrFin<(ApiContractClass, ClassHash)>,
    protobuf::ClassesResponse
);

impl TryFrom<protobuf::Class> for (ApiContractClass, ClassHash) {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::Class) -> Result<Self, Self::Error> {
        let class = match value.class {
            Some(protobuf::class::Class::Cairo0(class)) => {
                ApiContractClass::DeprecatedContractClass(
                    deprecated_contract_class::ContractClass::try_from(class)?,
                )
            }
            Some(protobuf::class::Class::Cairo1(class)) => {
                ApiContractClass::ContractClass(state::SierraContractClass::try_from(class)?)
            }
            None => {
                return Err(ProtobufConversionError::MissingField {
                    field_description: "Class::class",
                });
            }
        };
        let class_hash = value
            .class_hash
            .ok_or(ProtobufConversionError::MissingField {
                field_description: "Class::class_hash",
            })?
            .try_into()
            .map(ClassHash)?;
        Ok((class, class_hash))
    }
}

impl From<(ApiContractClass, ClassHash)> for protobuf::Class {
    fn from(value: (ApiContractClass, ClassHash)) -> Self {
        let (class, class_hash) = value;
        let domain = u32::try_from(volition_domain_to_enum_int(DOMAIN))
            .expect("volition_domain_to_enum_int output should be convertible to u32");
        let class = match class {
            ApiContractClass::DeprecatedContractClass(class) => {
                protobuf::class::Class::Cairo0(class.into())
            }
            ApiContractClass::ContractClass(class) => protobuf::class::Class::Cairo1(class.into()),
        };
        protobuf::Class { domain, class: Some(class), class_hash: Some(class_hash.0.into()) }
    }
}

impl TryFrom<protobuf::Cairo0Class> for deprecated_contract_class::ContractClass {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::Cairo0Class) -> Result<Self, Self::Error> {
        let mut entry_points_by_type = HashMap::new();

        if !value.constructors.is_empty() {
            entry_points_by_type.insert(
                EntryPointType::Constructor,
                value
                    .constructors
                    .into_iter()
                    .map(|entry_point| entry_point.try_into())
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }
        if !value.externals.is_empty() {
            entry_points_by_type.insert(
                EntryPointType::External,
                value
                    .externals
                    .into_iter()
                    .map(|entry_point| entry_point.try_into())
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }
        if !value.l1_handlers.is_empty() {
            entry_points_by_type.insert(
                EntryPointType::L1Handler,
                value
                    .l1_handlers
                    .into_iter()
                    .map(|entry_point| entry_point.try_into())
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }
        let abi = serde_json::from_str(&value.abi)?;
        let program = serde_json::from_value(decode_and_decompress(&value.program)?)?;

        Ok(Self { program, entry_points_by_type, abi })
    }
}

impl From<deprecated_contract_class::ContractClass> for protobuf::Cairo0Class {
    fn from(value: deprecated_contract_class::ContractClass) -> Self {
        // TODO: remove expects and handle results properly
        let serialized_program = serde_json::to_value(&value.program)
            .expect("Failed to serialize Cairo 0 program to serde_json::Value");

        // TODO: consider storing the encoded program
        let encoded_program = compress_and_encode(serialized_program)
            .expect("Failed to compress and encode serialized Cairo 0 program");

        // TODO: remove expects and handle results properly
        let encoded_abi = match value.abi {
            Some(abi_entries) => {
                let mut abi_bytes = vec![];
                abi_entries
                    .serialize(&mut serde_json::Serializer::with_formatter(
                        &mut abi_bytes,
                        PythonJsonFormatter,
                    ))
                    .expect("ABI is not in the expected Pythonic JSON byte format");
                String::from_utf8(abi_bytes).expect("Failed decoding ABI bytes as utf8 string")
            }
            None => "".to_string(),
        };

        protobuf::Cairo0Class {
            constructors: value
                .entry_points_by_type
                .get(&EntryPointType::Constructor)
                .unwrap_or(&vec![])
                .iter()
                .cloned()
                .map(protobuf::EntryPoint::from)
                .collect(),
            externals: value
                .entry_points_by_type
                .get(&EntryPointType::External)
                .unwrap_or(&vec![])
                .iter()
                .cloned()
                .map(protobuf::EntryPoint::from)
                .collect(),
            l1_handlers: value
                .entry_points_by_type
                .get(&EntryPointType::L1Handler)
                .unwrap_or(&vec![])
                .iter()
                .cloned()
                .map(protobuf::EntryPoint::from)
                .collect(),
            abi: encoded_abi,
            program: encoded_program,
        }
    }
}

impl TryFrom<protobuf::Cairo1Class> for state::SierraContractClass {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::Cairo1Class) -> Result<Self, Self::Error> {
        let abi = value.abi;

        let sierra_program =
            value.program.into_iter().map(Felt::try_from).collect::<Result<Vec<_>, _>>()?;

        let contract_class_version = value.contract_class_version;

        let mut entry_points_by_type = HashMap::new();
        let entry_points =
            value.entry_points.clone().ok_or(ProtobufConversionError::MissingField {
                field_description: "Cairo1Class::entry_points",
            })?;
        if !entry_points.constructors.is_empty() {
            entry_points_by_type.insert(
                EntryPointType::Constructor,
                entry_points
                    .constructors
                    .into_iter()
                    .map(|entry_point| entry_point.try_into())
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }
        if !entry_points.externals.is_empty() {
            entry_points_by_type.insert(
                EntryPointType::External,
                entry_points
                    .externals
                    .into_iter()
                    .map(|entry_point| entry_point.try_into())
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }
        if !entry_points.l1_handlers.is_empty() {
            entry_points_by_type.insert(
                EntryPointType::L1Handler,
                entry_points
                    .l1_handlers
                    .into_iter()
                    .map(|entry_point| entry_point.try_into())
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }

        Ok(state::SierraContractClass {
            sierra_program,
            entry_points_by_type,
            abi,
            contract_class_version,
        })
    }
}

impl From<state::SierraContractClass> for protobuf::Cairo1Class {
    fn from(value: state::SierraContractClass) -> Self {
        let abi = value.abi;

        let program =
            value.sierra_program.clone().into_iter().map(protobuf::Felt252::from).collect();

        let entry_points = Some(protobuf::Cairo1EntryPoints {
            constructors: value
                .entry_points_by_type
                .get(&EntryPointType::Constructor)
                .unwrap_or(&vec![])
                .iter()
                .cloned()
                .map(protobuf::SierraEntryPoint::from)
                .collect(),

            externals: value
                .entry_points_by_type
                .get(&EntryPointType::External)
                .unwrap_or(&vec![])
                .iter()
                .cloned()
                .map(protobuf::SierraEntryPoint::from)
                .collect(),
            l1_handlers: value
                .entry_points_by_type
                .get(&EntryPointType::L1Handler)
                .unwrap_or(&vec![])
                .iter()
                .cloned()
                .map(protobuf::SierraEntryPoint::from)
                .collect(),
        });

        let contract_class_version = format!(
            "sierra-v{}.{}.{} cairo-v{}.{}.{}",
            value.sierra_program[0],
            value.sierra_program[1],
            value.sierra_program[2],
            value.sierra_program[3],
            value.sierra_program[4],
            value.sierra_program[5]
        );

        protobuf::Cairo1Class { abi, program, entry_points, contract_class_version }
    }
}

impl TryFrom<protobuf::EntryPoint> for deprecated_contract_class::EntryPointV0 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::EntryPoint) -> Result<Self, Self::Error> {
        let selector_felt =
            Felt::try_from(value.selector.ok_or(ProtobufConversionError::MissingField {
                field_description: "EntryPoint::selector",
            })?)?;
        let selector = EntryPointSelector(selector_felt);

        let offset = deprecated_contract_class::EntryPointOffset(
            value.offset.try_into().expect("Failed converting u64 to usize"),
        );

        Ok(deprecated_contract_class::EntryPointV0 { selector, offset })
    }
}

impl From<deprecated_contract_class::EntryPointV0> for protobuf::EntryPoint {
    fn from(value: deprecated_contract_class::EntryPointV0) -> Self {
        protobuf::EntryPoint {
            selector: Some(value.selector.0.into()),
            offset: u64::try_from(value.offset.0).expect("Failed converting usize to u64"),
        }
    }
}

impl TryFrom<protobuf::SierraEntryPoint> for state::EntryPoint {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::SierraEntryPoint) -> Result<Self, Self::Error> {
        let selector_felt =
            Felt::try_from(value.selector.ok_or(ProtobufConversionError::MissingField {
                field_description: "SierraEntryPoint::selector",
            })?)?;
        let selector = EntryPointSelector(selector_felt);

        let function_idx =
            state::FunctionIndex(value.index.try_into().expect("Failed converting u64 to usize"));

        Ok(state::EntryPoint { function_idx, selector })
    }
}

impl From<state::EntryPoint> for protobuf::SierraEntryPoint {
    fn from(value: state::EntryPoint) -> Self {
        protobuf::SierraEntryPoint {
            index: u64::try_from(value.function_idx.0).expect("Failed converting usize to u64"),
            selector: Some(value.selector.0.into()),
        }
    }
}

impl TryFrom<protobuf::ClassesRequest> for Query {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::ClassesRequest) -> Result<Self, Self::Error> {
        Ok(ClassQuery::try_from(value)?.0)
    }
}

impl TryFrom<protobuf::ClassesRequest> for ClassQuery {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::ClassesRequest) -> Result<Self, Self::Error> {
        Ok(ClassQuery(
            value
                .iteration
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "ClassesRequest::iteration",
                })?
                .try_into()?,
        ))
    }
}

impl From<Query> for protobuf::ClassesRequest {
    fn from(value: Query) -> Self {
        protobuf::ClassesRequest { iteration: Some(value.into()) }
    }
}

impl From<ClassQuery> for protobuf::ClassesRequest {
    fn from(value: ClassQuery) -> Self {
        protobuf::ClassesRequest { iteration: Some(value.0.into()) }
    }
}

auto_impl_into_and_try_from_vec_u8!(ClassQuery, protobuf::ClassesRequest);
