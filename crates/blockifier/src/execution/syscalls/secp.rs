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
use crate::execution::syscalls::hint_processor::felt_to_bool;
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
    points_segment_base: Option<Relocatable>,
}

impl<Curve: SWCurveConfig> SecpHintProcessor<Curve>
where
    Curve::BaseField: PrimeField,
{
    pub fn new() -> Self {
        Self { points: Vec::default(), points_segment_base: None }
    }

    pub fn secp_add(&mut self, request: SecpAddRequest) -> SyscallResult<SecpAddResponse> {
        let lhs = self.get_point_by_id(request.lhs_id)?;
        let rhs = self.get_point_by_id(request.rhs_id)?;
        let result = *lhs + *rhs;
        let ec_point_ptr = self.allocate_point(result.into());
        Ok(SecpOpRespone { ec_point_ptr })
    }

    pub fn secp_mul(&mut self, request: SecpMulRequest) -> SyscallResult<SecpMulResponse> {
        let ec_point = self.get_point_by_id(request.ec_point_id)?;
        let result = *ec_point * Curve::ScalarField::from(request.multiplier);
        let ec_point_ptr = self.allocate_point(result.into());
        Ok(SecpOpRespone { ec_point_ptr })
    }

    pub fn secp_get_point_from_x(
        &mut self,
        request: SecpGetPointFromXRequest,
    ) -> SyscallResult<SecpGetPointFromXResponse> {
        let affine = crate::execution::secp::get_point_from_x(request.x, request.y_parity)?;
        Ok(SecpGetPointFromXResponse {
            optional_ec_point_ptr: affine.map(|ec_point| self.allocate_point(ec_point)),
        })
    }

    pub fn secp_get_xy(&mut self, request: SecpGetXyRequest) -> SyscallResult<SecpGetXyResponse> {
        let ec_point = self.get_point_by_id(request.ec_point_id)?;

        Ok(SecpGetXyResponse { x: ec_point.x.into(), y: ec_point.y.into() })
    }

    pub fn secp_new(
        &mut self,
        vm: &mut VirtualMachine,
        request: SecpNewRequest,
    ) -> SyscallResult<SecpNewResponse> {
        if self.points_segment_base.is_none() {
            self.points_segment_base = Some(vm.add_memory_segment());
        }
        let affine = new_affine::<Curve>(request.x, request.y)?;

        Ok(SecpNewResponse {
            optional_ec_point_ptr: affine.map(|ec_point| self.allocate_point(ec_point)),
        })
    }

    fn allocate_point(&mut self, ec_point: short_weierstrass::Affine<Curve>) -> Relocatable {
        let points = &mut self.points;
        let id = points.len();
        points.push(ec_point);

        // TODO!(Aner): replace unwrap with Result.
        (self.points_segment_base.expect("segments should be already initialized.") + 6 * id)
            .unwrap()
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
    pub ec_point_ptr: Relocatable,
}

#[derive(Debug, Eq, PartialEq)]
pub struct SecpOptionalEcPointResponse {
    // `Option<SecpPoint>` which is represented as two felts.
    // The first felt is a indicates if it is `Some` (0) or `None` (1).
    // The second felt is only valid if the first felt is `Some` and contains the ID of the point.
    // The ID allocated by the Secp hint processor.
    pub optional_ec_point_ptr: Option<Relocatable>,
}

impl SyscallResponse for SecpOptionalEcPointResponse {
    fn write(self, vm: &mut VirtualMachine, ptr: &mut Relocatable) -> WriteResponseResult {
        match self.optional_ec_point_ptr {
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
        write_maybe_relocatable(vm, ptr, self.ec_point_ptr)?;
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

pub type SecpAddResponse = SecpOpRespone;

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

pub type SecpGetPointFromXResponse = SecpOptionalEcPointResponse;

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

pub type SecpGetXyResponse = EcPointCoordinates;

impl SyscallResponse for SecpGetXyResponse {
    fn write(self, vm: &mut VirtualMachine, ptr: &mut Relocatable) -> WriteResponseResult {
        write_u256(vm, ptr, self.x)?;
        write_u256(vm, ptr, self.y)?;
        Ok(())
    }
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

pub type SecpMulResponse = SecpOpRespone;

// SecpNew syscall.

pub type SecpNewRequest = EcPointCoordinates;

impl SyscallRequest for SecpNewRequest {
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallResult<SecpNewRequest> {
        let x = SierraU256::from_memory(vm, ptr)?.to_biguint();
        let y = SierraU256::from_memory(vm, ptr)?.to_biguint();
        Ok(SecpNewRequest { x, y })
    }
}

pub type SecpNewResponse = SecpOptionalEcPointResponse;
pub type Secp256r1NewRequest = EcPointCoordinates;
pub type Secp256r1NewResponse = SecpOptionalEcPointResponse;
