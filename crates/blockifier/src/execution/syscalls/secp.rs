use std::collections::HashMap;

use ark_ec::short_weierstrass::{self, SWCurveConfig};
use ark_ff::PrimeField;
use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::vm_core::VirtualMachine;
use num_bigint::BigUint;

use crate::abi::sierra_types::{SierraType, SierraU256};
use crate::execution::execution_utils::{
    felt_from_ptr,
    relocatable_from_ptr,
    write_maybe_relocatable,
    write_u256,
};
use crate::execution::secp::new_affine;
use crate::execution::syscalls::hint_processor::felt_to_bool;
use crate::execution::syscalls::vm_syscall_utils::{
    SyscallBaseResult,
    SyscallExecutorBaseError,
    SyscallRequest,
    SyscallResponse,
    WriteResponseResult,
};

const EC_POINT_SEGMENT_SIZE: usize = 6;

#[derive(Debug, Default, Eq, PartialEq)]
pub struct SecpHintProcessor<Curve: SWCurveConfig> {
    pub points: HashMap<Relocatable, short_weierstrass::Affine<Curve>>,
}

impl<Curve: SWCurveConfig> SecpHintProcessor<Curve>
where
    Curve::BaseField: PrimeField,
{
    pub fn new() -> Self {
        Self { points: HashMap::new() }
    }

    pub fn secp_add(
        &mut self,
        request: SecpAddRequest,
        vm: &mut VirtualMachine,
        points_segment_base: Relocatable,
        id: usize,
    ) -> SyscallBaseResult<SecpAddResponse> {
        let lhs = self.get_point_by_ptr(request.lhs_ptr)?;
        let rhs = self.get_point_by_ptr(request.rhs_ptr)?;
        let result = *lhs + *rhs;
        let ec_point_ptr =
            self.allocate_point(result.into(), vm, &mut Some(points_segment_base), id)?;
        Ok(SecpOpRespone { ec_point_ptr })
    }

    pub fn secp_mul(
        &mut self,
        request: SecpMulRequest,
        vm: &mut VirtualMachine,
        points_segment_base: Relocatable,
        id: usize,
    ) -> SyscallBaseResult<SecpMulResponse> {
        let ec_point = self.get_point_by_ptr(request.ec_point_ptr)?;
        let result = *ec_point * Curve::ScalarField::from(request.multiplier);
        let ec_point_ptr =
            self.allocate_point(result.into(), vm, &mut Some(points_segment_base), id)?;
        Ok(SecpOpRespone { ec_point_ptr })
    }

    pub fn secp_get_point_from_x(
        &mut self,
        vm: &mut VirtualMachine,
        request: SecpGetPointFromXRequest,
        points_segment_base: &mut Option<Relocatable>,
        id: usize,
    ) -> SyscallBaseResult<SecpGetPointFromXResponse> {
        let affine = crate::execution::secp::get_point_from_x(request.x, request.y_parity)?;
        let optional_ec_point_ptr = match affine
            .map(|ec_point| self.allocate_point(ec_point, vm, points_segment_base, id))
        {
            Some(Ok(ptr)) => Some(ptr),
            Some(Err(err)) => return Err(err),
            None => None,
        };
        Ok(SecpOptionalEcPointResponse { optional_ec_point_ptr })
    }

    pub fn secp_get_xy(&self, request: SecpGetXyRequest) -> SyscallBaseResult<SecpGetXyResponse> {
        let ec_point = self.get_point_by_ptr(request.ec_point_ptr)?;
        Ok(SecpGetXyResponse { x: ec_point.x.into(), y: ec_point.y.into() })
    }

    pub fn secp_new(
        &mut self,
        vm: &mut VirtualMachine,
        request: SecpNewRequest,
        points_segment_base: &mut Option<Relocatable>,
        id: usize,
    ) -> SyscallBaseResult<SecpNewResponse> {
        let affine = new_affine::<Curve>(request.x, request.y)?;
        let optional_ec_point_ptr = match affine
            .map(|ec_point| self.allocate_point(ec_point, vm, points_segment_base, id))
        {
            Some(Ok(ptr)) => Some(ptr),
            Some(Err(err)) => return Err(err),
            None => None,
        };
        Ok(SecpNewResponse { optional_ec_point_ptr })
    }

    fn allocate_point(
        &mut self,
        ec_point: short_weierstrass::Affine<Curve>,
        vm: &mut VirtualMachine,
        points_segment_base: &mut Option<Relocatable>,
        id: usize,
    ) -> SyscallBaseResult<Relocatable> {
        if points_segment_base.is_none() {
            *points_segment_base = Some(vm.add_memory_segment());
        }
        let point_address = (points_segment_base.expect("Points segment base must be set.")
            + EC_POINT_SEGMENT_SIZE * id)?;
        self.points.insert(point_address, ec_point);
        Ok(point_address)
    }

    fn get_point_by_ptr(
        &self,
        ec_point_ptr: Relocatable,
    ) -> SyscallBaseResult<&short_weierstrass::Affine<Curve>> {
        self.points.get(&ec_point_ptr).ok_or_else(|| {
            SyscallExecutorBaseError::InvalidSyscallInput {
                input: ec_point_ptr.segment_index.into(),
                info: "Invalid Secp point address".to_string(),
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
    // The first felt indicates if it is `Some` (0) or `None` (1). The second felt is only valid if
    // the first felt is `Some` and contains a pointer to the ec_point in the vm.
    pub optional_ec_point_ptr: Option<Relocatable>,
}

impl SyscallResponse for SecpOptionalEcPointResponse {
    fn write(self, vm: &mut VirtualMachine, ptr: &mut Relocatable) -> WriteResponseResult {
        match self.optional_ec_point_ptr {
            Some(ec_point_ptr) => {
                // Cairo 1 representation of Some(ptr).
                write_maybe_relocatable(vm, ptr, 0)?;
                write_maybe_relocatable(vm, ptr, ec_point_ptr)?;
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
    pub lhs_ptr: Relocatable,
    pub rhs_ptr: Relocatable,
}

impl SyscallRequest for SecpAddRequest {
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallBaseResult<SecpAddRequest> {
        Ok(SecpAddRequest {
            lhs_ptr: relocatable_from_ptr(vm, ptr)?,
            rhs_ptr: relocatable_from_ptr(vm, ptr)?,
        })
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
    fn read(
        vm: &VirtualMachine,
        ptr: &mut Relocatable,
    ) -> SyscallBaseResult<SecpGetPointFromXRequest> {
        let x = SierraU256::from_memory(vm, ptr)?.to_biguint();

        let y_parity = felt_to_bool(felt_from_ptr(vm, ptr)?, "Invalid y parity")?;
        Ok(SecpGetPointFromXRequest { x, y_parity })
    }
}

pub type SecpGetPointFromXResponse = SecpOptionalEcPointResponse;

// SecpGetXy syscall.

#[derive(Debug, Eq, PartialEq)]
pub struct SecpGetXyRequest {
    pub ec_point_ptr: Relocatable,
}

impl SyscallRequest for SecpGetXyRequest {
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallBaseResult<SecpGetXyRequest> {
        Ok(SecpGetXyRequest { ec_point_ptr: relocatable_from_ptr(vm, ptr)? })
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
    pub ec_point_ptr: Relocatable,
    pub multiplier: BigUint,
}

impl SyscallRequest for SecpMulRequest {
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallBaseResult<SecpMulRequest> {
        let ec_point_ptr = relocatable_from_ptr(vm, ptr)?;
        let multiplier = SierraU256::from_memory(vm, ptr)?.to_biguint();
        Ok(SecpMulRequest { ec_point_ptr, multiplier })
    }
}

pub type SecpMulResponse = SecpOpRespone;

// SecpNew syscall.

pub type SecpNewRequest = EcPointCoordinates;

impl SyscallRequest for SecpNewRequest {
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallBaseResult<SecpNewRequest> {
        let x = SierraU256::from_memory(vm, ptr)?.to_biguint();
        let y = SierraU256::from_memory(vm, ptr)?.to_biguint();
        Ok(SecpNewRequest { x, y })
    }
}

pub type SecpNewResponse = SecpOptionalEcPointResponse;
pub type Secp256r1NewRequest = EcPointCoordinates;
pub type Secp256r1NewResponse = SecpOptionalEcPointResponse;
