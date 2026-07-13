//! Validation of the entry point return types of a declared Sierra class, as performed by the
//! Sierra-to-CASM compiler.
//!
//! The code is copied from `cairo_lang_starknet_classes::casm_contract_class` (the version pinned
//! in the workspace) and intentionally mirrors it as closely as possible to ease diffing against
//! the original. The deliberate divergences are:
//! - Only the entry point return type validation is kept; the rest of the signature checks (which
//!   compilation re-runs anyway), compilation, hashing and CASM entry point construction are
//!   dropped.
//! - The panic type of an entry point's error variant must be the empty struct (a fix not yet
//!   present in the pinned compiler version).
//!
//! As in the original, `TypeResolver` panics on a malformed program (out-of-range type ids), so
//! this validation must only run on classes that already compiled successfully.

use apollo_compilation_utils::class_utils::into_contract_class_for_compilation;
use cairo_lang_sierra::extensions::array::ArrayType;
use cairo_lang_sierra::extensions::enm::EnumType;
use cairo_lang_sierra::extensions::felt252::Felt252Type;
use cairo_lang_sierra::extensions::snapshot::SnapshotType;
use cairo_lang_sierra::extensions::structure::StructType;
use cairo_lang_sierra::extensions::NamedType;
use cairo_lang_sierra::ids::{ConcreteTypeId, GenericTypeId};
use cairo_lang_sierra::program::{ConcreteTypeLongId, GenericArg, TypeDeclaration};
use cairo_lang_starknet_classes::contract_class::ContractEntryPoint;
use cairo_lang_utils::require;
use starknet_api::state::SierraContractClass;
use thiserror::Error;

// Trimmed copy of `cairo_lang_starknet_classes::casm_contract_class`'s error enum of the same
// name, keeping only the variants produced by the entry point return type validation, plus a
// variant for a Sierra program that fails to deserialize (the compiler fails earlier in that
// case, before reaching this validation).
#[derive(Error, Debug, Eq, PartialEq)]
pub enum StarknetSierraCompilationError {
    #[error("Failed deserializing the Sierra program: {0}")]
    SierraProgramDeserializationFailed(String),
    #[error("Invalid entry point.")]
    EntryPointError,
    #[error("Invalid entry point signature.")]
    InvalidEntryPointSignature,
}

/// Validates the return type of every entry point of the given class, as the Sierra-to-CASM
/// compiler (`CasmContractClass::from_contract_class`) does before compiling it.
pub fn validate_entry_point_return_types(
    contract_class: &SierraContractClass,
) -> Result<(), StarknetSierraCompilationError> {
    let contract_class = into_contract_class_for_compilation(contract_class);
    let extracted_program = contract_class.extract_sierra_program(false).map_err(|error| {
        StarknetSierraCompilationError::SierraProgramDeserializationFailed(error.to_string())
    })?;
    let program = extracted_program.program;

    let validate_entry_point = |contract_entry_point: &ContractEntryPoint| {
        let Some(function) = program.funcs.get(contract_entry_point.function_idx) else {
            return Err(StarknetSierraCompilationError::EntryPointError);
        };

        // The expected return types are [builtins.., gas_builtin, system, PanicResult].
        let (panic_result, _output_builtins) = function
            .signature
            .ret_types
            .split_last()
            .ok_or(StarknetSierraCompilationError::InvalidEntryPointSignature)?;

        let type_resolver = TypeResolver { type_decl: &program.type_declarations };
        require(type_resolver.is_valid_entry_point_return_type(panic_result))
            .ok_or(StarknetSierraCompilationError::InvalidEntryPointSignature)?;

        Ok(())
    };

    for entry_point in contract_class
        .entry_points_by_type
        .constructor
        .iter()
        .chain(contract_class.entry_points_by_type.external.iter())
        .chain(contract_class.entry_points_by_type.l1_handler.iter())
    {
        validate_entry_point(entry_point)?;
    }

    Ok(())
}

/// Context for resolving types.
pub struct TypeResolver<'a> {
    type_decl: &'a [TypeDeclaration],
}

impl TypeResolver<'_> {
    fn get_long_id(&self, type_id: &ConcreteTypeId) -> &ConcreteTypeLongId {
        &self.type_decl[type_id.id as usize].long_id
    }

    fn get_generic_id(&self, type_id: &ConcreteTypeId) -> &GenericTypeId {
        &self.get_long_id(type_id).generic_id
    }

    fn is_felt252_array_snapshot(&self, ty: &ConcreteTypeId) -> bool {
        let long_id = self.get_long_id(ty);
        if long_id.generic_id != SnapshotType::id() {
            return false;
        }

        let [GenericArg::Type(inner_ty)] = long_id.generic_args.as_slice() else {
            return false;
        };

        self.is_felt252_array(inner_ty)
    }

    fn is_felt252_array(&self, ty: &ConcreteTypeId) -> bool {
        let long_id = self.get_long_id(ty);
        if long_id.generic_id != ArrayType::id() {
            return false;
        }

        let [GenericArg::Type(element_ty)] = long_id.generic_args.as_slice() else {
            return false;
        };

        *self.get_generic_id(element_ty) == Felt252Type::id()
    }

    fn is_felt252_span(&self, ty: &ConcreteTypeId) -> bool {
        let long_id = self.get_long_id(ty);
        if long_id.generic_id != StructType::ID {
            return false;
        }

        let [GenericArg::UserType(_), GenericArg::Type(element_ty)] =
            long_id.generic_args.as_slice()
        else {
            return false;
        };

        self.is_felt252_array_snapshot(element_ty)
    }

    fn is_valid_entry_point_return_type(&self, ty: &ConcreteTypeId) -> bool {
        // The return type must be an enum with two variants: (result, error).
        let Some((result_tuple_ty, err_ty)) = self.extract_result_ty(ty) else {
            return false;
        };

        // The result variant must be a tuple with one element: Span<felt252>;
        let Some(result_ty) = self.extract_struct1(result_tuple_ty) else {
            return false;
        };
        if !self.is_felt252_span(result_ty) {
            return false;
        }

        // If the error type is Array<felt252>, it's a good error type, using the old panic
        // mechanism.
        if self.is_felt252_array(err_ty) {
            return true;
        }

        // Otherwise, the error type must be a struct with two fields: (panic, data)
        let Some((panic_ty, err_data_ty)) = self.extract_struct2(err_ty) else {
            return false;
        };

        // The panic field must be the empty struct, as the entry point ABI expects the error
        // variant to be laid out as an `Array<felt252>` only.
        if !self.is_empty_struct(panic_ty) {
            return false;
        }

        // The data field must be an Array<felt252>.
        self.is_felt252_array(err_data_ty)
    }

    /// Returns true if the type is the empty struct: a struct without any members.
    fn is_empty_struct(&self, ty: &ConcreteTypeId) -> bool {
        let long_id = self.get_long_id(ty);
        if long_id.generic_id != StructType::id() {
            return false;
        }
        let [GenericArg::UserType(_), members @ ..] = long_id.generic_args.as_slice() else {
            return false;
        };
        members.is_empty()
    }

    /// Extracts types `TOk`, `TErr` from the type `Result<TOk, TErr>`.
    fn extract_result_ty(&self, ty: &ConcreteTypeId) -> Option<(&ConcreteTypeId, &ConcreteTypeId)> {
        let long_id = self.get_long_id(ty);
        require(long_id.generic_id == EnumType::id())?;
        let [GenericArg::UserType(_), GenericArg::Type(result_tuple_ty), GenericArg::Type(err_ty)] =
            long_id.generic_args.as_slice()
        else {
            return None;
        };
        Some((result_tuple_ty, err_ty))
    }

    /// Extracts type `T` from the tuple type `(T,)`.
    fn extract_struct1(&self, ty: &ConcreteTypeId) -> Option<&ConcreteTypeId> {
        let long_id = self.get_long_id(ty);
        require(long_id.generic_id == StructType::id())?;
        let [GenericArg::UserType(_), GenericArg::Type(ty0)] = long_id.generic_args.as_slice()
        else {
            return None;
        };
        Some(ty0)
    }

    /// Extracts types `T0`, `T1` from the tuple type `(T0, T1)`.
    fn extract_struct2(&self, ty: &ConcreteTypeId) -> Option<(&ConcreteTypeId, &ConcreteTypeId)> {
        let long_id = self.get_long_id(ty);
        require(long_id.generic_id == StructType::id())?;
        let [GenericArg::UserType(_), GenericArg::Type(ty0), GenericArg::Type(ty1)] =
            long_id.generic_args.as_slice()
        else {
            return None;
        };
        Some((ty0, ty1))
    }
}
