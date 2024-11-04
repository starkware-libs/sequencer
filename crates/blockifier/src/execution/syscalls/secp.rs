use ark_ec::short_weierstrass::{self, SWCurveConfig};
use ark_ff::PrimeField;
use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::vm_core::VirtualMachine;
use num_bigint::BigUint;
use num_traits::ToPrimitive;
use starknet_types_core::felt::Felt;

use crate::abi::sierra_types::{SierraType, SierraU256};
use crate::execution::execution_utils::{felt_from_ptr, write_maybe_relocatable, write_u256};
use crate::execution::secp::new_affine;
use crate::execution::syscalls::hint_processor::{felt_to_bool, SyscallHintProcessor};
use crate::execution::syscalls::{
    SyscallExecutionError,
    SyscallRequest,
    SyscallResponse,
    SyscallResult,
    WriteResponseResult,
};

#[derive(Debug, Default, Eq, PartialEq)]
pub struct SecpHintProcessor<Curve: SWCurveConfig> {
    points: Vec<short_weierstrass::Affine<Curve>>,
}

impl<Curve: SWCurveConfig> SecpHintProcessor<Curve>
where
    Curve::BaseField: PrimeField,
{
    pub fn secp_add(&mut self, request: SecpAddRequest) -> SyscallResult<SecpAddResponse> {
        let lhs = self.get_point_by_id(request.lhs_id)?;
        let rhs = self.get_point_by_id(request.rhs_id)?;
        let result = *lhs + *rhs;
        let ec_point_id = self.allocate_point(result.into());
        Ok(SecpOpRespone { ec_point_id })
    }

    pub fn secp_mul(&mut self, request: SecpMulRequest) -> SyscallResult<SecpMulResponse> {
        let ec_point = self.get_point_by_id(request.ec_point_id)?;
        let result = *ec_point * Curve::ScalarField::from(request.multiplier);
        let ec_point_id = self.allocate_point(result.into());
        Ok(SecpOpRespone { ec_point_id })
    }

    pub fn secp_get_point_from_x(
        &mut self,
        request: SecpGetPointFromXRequest,
    ) -> SyscallResult<SecpGetPointFromXResponse> {
        let affine = crate::execution::secp::get_point_from_x(request.x, request.y_parity);

        affine.map(|maybe_ec_point| SecpGetPointFromXResponse {
            optional_ec_point_id: maybe_ec_point.map(|ec_point| self.allocate_point(ec_point)),
        })
    }

    pub fn secp_get_xy(&mut self, request: SecpGetXyRequest) -> SyscallResult<SecpGetXyResponse> {
        let ec_point = self.get_point_by_id(request.ec_point_id)?;

        Ok(SecpGetXyResponse { x: ec_point.x.into(), y: ec_point.y.into() })
    }

    pub fn secp_new(&mut self, request: SecpNewRequest) -> SyscallResult<SecpNewResponse> {
        let affine = new_affine::<Curve>(request.x, request.y);
        affine.map(|maybe_ec_point| SecpNewResponse {
            optional_ec_point_id: maybe_ec_point.map(|point| self.allocate_point(point)),
        })
    }

    fn allocate_point(&mut self, ec_point: short_weierstrass::Affine<Curve>) -> usize {
        let points = &mut self.points;
        let id = points.len();
        points.push(ec_point);
        id
    }

    fn get_point_by_id(
        &self,
        ec_point_id: Felt,
    ) -> SyscallResult<&short_weierstrass::Affine<Curve>> {
        ec_point_id.to_usize().and_then(|id| self.points.get(id)).ok_or_else(|| {
            SyscallExecutionError::InvalidSyscallInput {
                input: ec_point_id,
                info: "Invalid Secp point ID".to_string(),
            }
        })
    }
}

// The x and y coordinates of an elliptic curve point.
#[derive(Debug, Eq, PartialEq)]
pub struct EcPointCoordinates {
    pub x: BigUint,
    pub y: BigUint,
}

#[derive(Debug, Eq, PartialEq)]
pub struct SecpOpRespone {
    pub ec_point_id: usize,
}

#[derive(Debug, Eq, PartialEq)]
pub struct SecpOptionalEcPointResponse {
    // `Option<SecpPoint>` which is represented as two felts.
    // The first felt is a indicates if it is `Some` (0) or `None` (1).
    // The second felt is only valid if the first felt is `Some` and contains the ID of the point.
    // The ID allocated by the Secp hint processor.
    pub optional_ec_point_id: Option<usize>,
}

impl SyscallResponse for SecpOptionalEcPointResponse {
    fn write(self, vm: &mut VirtualMachine, ptr: &mut Relocatable) -> WriteResponseResult {
        match self.optional_ec_point_id {
            Some(id) => {
                // Cairo 1 representation of Some(id).
                write_maybe_relocatable(vm, ptr, 0)?;
                write_maybe_relocatable(vm, ptr, id)?;
            }
            None => {
                // Cairo 1 representation of None.
                write_maybe_relocatable(vm, ptr, 1)?;
                write_maybe_relocatable(vm, ptr, 0)?;
            }
        };
        Ok(())
    }
}

impl SyscallResponse for SecpOpRespone {
    fn write(self, vm: &mut VirtualMachine, ptr: &mut Relocatable) -> WriteResponseResult {
        write_maybe_relocatable(vm, ptr, self.ec_point_id)?;
        Ok(())
    }
}

// SecpAdd syscall.

#[derive(Debug, Eq, PartialEq)]
pub struct SecpAddRequest {
    pub lhs_id: Felt,
    pub rhs_id: Felt,
}

impl SyscallRequest for SecpAddRequest {
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallResult<SecpAddRequest> {
        Ok(SecpAddRequest { lhs_id: felt_from_ptr(vm, ptr)?, rhs_id: felt_from_ptr(vm, ptr)? })
    }
}

type SecpAddResponse = SecpOpRespone;

pub fn secp256k1_add(
    request: SecpAddRequest,
    _vm: &mut VirtualMachine,
    syscall_handler: &mut SyscallHintProcessor<'_>,
    _remaining_gas: &mut u64,
) -> SyscallResult<SecpOpRespone> {
    syscall_handler.secp256k1_hint_processor.secp_add(request)
}

pub fn secp256r1_add(
    request: SecpAddRequest,
    _vm: &mut VirtualMachine,
    syscall_handler: &mut SyscallHintProcessor<'_>,
    _remaining_gas: &mut u64,
) -> SyscallResult<SecpOpRespone> {
    syscall_handler.secp256r1_hint_processor.secp_add(request)
}

// SecpGetPointFromXRequest syscall.

#[derive(Debug, Eq, PartialEq)]
pub struct SecpGetPointFromXRequest {
    x: BigUint,
    // The parity of the y coordinate, assuming a point with the given x coordinate exists.
    // True means the y coordinate is odd.
    y_parity: bool,
}

impl SyscallRequest for SecpGetPointFromXRequest {
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallResult<SecpGetPointFromXRequest> {
        let x = SierraU256::from_memory(vm, ptr)?.to_biguint();

        let y_parity = felt_to_bool(felt_from_ptr(vm, ptr)?, "Invalid y parity")?;
        Ok(SecpGetPointFromXRequest { x, y_parity })
    }
}

type SecpGetPointFromXResponse = SecpOptionalEcPointResponse;

pub fn secp256k1_get_point_from_x(
    request: SecpGetPointFromXRequest,
    _vm: &mut VirtualMachine,
    syscall_handler: &mut SyscallHintProcessor<'_>,
    _remaining_gas: &mut u64,
) -> SyscallResult<SecpGetPointFromXResponse> {
    syscall_handler.secp256k1_hint_processor.secp_get_point_from_x(request)
}

pub fn secp256r1_get_point_from_x(
    request: SecpGetPointFromXRequest,
    _vm: &mut VirtualMachine,
    syscall_handler: &mut SyscallHintProcessor<'_>,
    _remaining_gas: &mut u64,
) -> SyscallResult<SecpGetPointFromXResponse> {
    syscall_handler.secp256r1_hint_processor.secp_get_point_from_x(request)
}

// SecpGetXy syscall.

#[derive(Debug, Eq, PartialEq)]
pub struct SecpGetXyRequest {
    pub ec_point_id: Felt,
}

impl SyscallRequest for SecpGetXyRequest {
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallResult<SecpGetXyRequest> {
        Ok(SecpGetXyRequest { ec_point_id: felt_from_ptr(vm, ptr)? })
    }
}

type SecpGetXyResponse = EcPointCoordinates;

impl SyscallResponse for SecpGetXyResponse {
    fn write(self, vm: &mut VirtualMachine, ptr: &mut Relocatable) -> WriteResponseResult {
        write_u256(vm, ptr, self.x)?;
        write_u256(vm, ptr, self.y)?;
        Ok(())
    }
}

pub fn secp256k1_get_xy(
    request: SecpGetXyRequest,
    _vm: &mut VirtualMachine,
    syscall_handler: &mut SyscallHintProcessor<'_>,
    _remaining_gas: &mut u64,
) -> SyscallResult<SecpGetXyResponse> {
    syscall_handler.secp256k1_hint_processor.secp_get_xy(request)
}

pub fn secp256r1_get_xy(
    request: SecpGetXyRequest,
    _vm: &mut VirtualMachine,
    syscall_handler: &mut SyscallHintProcessor<'_>,
    _remaining_gas: &mut u64,
) -> SyscallResult<SecpGetXyResponse> {
    syscall_handler.secp256r1_hint_processor.secp_get_xy(request)
}

// SecpMul syscall.

#[derive(Debug, Eq, PartialEq)]
pub struct SecpMulRequest {
    pub ec_point_id: Felt,
    pub multiplier: BigUint,
}

impl SyscallRequest for SecpMulRequest {
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallResult<SecpMulRequest> {
        let ec_point_id = felt_from_ptr(vm, ptr)?;
        let multiplier = SierraU256::from_memory(vm, ptr)?.to_biguint();
        Ok(SecpMulRequest { ec_point_id, multiplier })
    }
}

type SecpMulResponse = SecpOpRespone;

pub fn secp256k1_mul(
    request: SecpMulRequest,
    _vm: &mut VirtualMachine,
    syscall_handler: &mut SyscallHintProcessor<'_>,
    _remaining_gas: &mut u64,
) -> SyscallResult<SecpMulResponse> {
    syscall_handler.secp256k1_hint_processor.secp_mul(request)
}

pub fn secp256r1_mul(
    request: SecpMulRequest,
    _vm: &mut VirtualMachine,
    syscall_handler: &mut SyscallHintProcessor<'_>,
    _remaining_gas: &mut u64,
) -> SyscallResult<SecpMulResponse> {
    syscall_handler.secp256r1_hint_processor.secp_mul(request)
}

// SecpNew syscall.

type SecpNewRequest = EcPointCoordinates;

impl SyscallRequest for SecpNewRequest {
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallResult<SecpNewRequest> {
        let x = SierraU256::from_memory(vm, ptr)?.to_biguint();
        let y = SierraU256::from_memory(vm, ptr)?.to_biguint();
        Ok(SecpNewRequest { x, y })
    }
}

type SecpNewResponse = SecpOptionalEcPointResponse;

pub fn secp256k1_new(
    request: SecpNewRequest,
    _vm: &mut VirtualMachine,
    syscall_handler: &mut SyscallHintProcessor<'_>,
    _remaining_gas: &mut u64,
) -> SyscallResult<SecpNewResponse> {
    syscall_handler.secp256k1_hint_processor.secp_new(request)
}

type Secp256r1NewRequest = EcPointCoordinates;
type Secp256r1NewResponse = SecpOptionalEcPointResponse;

pub fn secp256r1_new(
    request: Secp256r1NewRequest,
    _vm: &mut VirtualMachine,
    syscall_handler: &mut SyscallHintProcessor<'_>,
    _remaining_gas: &mut u64,
) -> SyscallResult<Secp256r1NewResponse> {
    syscall_handler.secp256r1_hint_processor.secp_new(request)
}
