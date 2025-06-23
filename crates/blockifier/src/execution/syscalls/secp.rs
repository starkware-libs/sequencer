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

    #[allow(clippy::result_large_err)]
    pub fn secp_add(
        &mut self,
        request: SecpAddRequest,
        vm: &mut VirtualMachine,
    ) -> SyscallBaseResult<SecpAddResponse> {
        let lhs = self.get_point_by_ptr(request.lhs_ptr)?;
        let rhs = self.get_point_by_ptr(request.rhs_ptr)?;
        let result = *lhs + *rhs;
        let ec_point_ptr = self.allocate_point(result.into(), vm)?;
        Ok(SecpOpRespone { ec_point_ptr })
    }

    #[allow(clippy::result_large_err)]
    pub fn secp_mul(
        &mut self,
        request: SecpMulRequest,
        vm: &mut VirtualMachine,
    ) -> SyscallBaseResult<SecpMulResponse> {
        let ec_point = self.get_point_by_ptr(request.ec_point_ptr)?;
        let result = *ec_point * Curve::ScalarField::from(request.multiplier);
        let ec_point_ptr = self.allocate_point(result.into(), vm)?;
        Ok(SecpOpRespone { ec_point_ptr })
    }

    #[allow(clippy::result_large_err)]
    pub fn secp_get_point_from_x(
        &mut self,
        vm: &mut VirtualMachine,
        request: SecpGetPointFromXRequest,
    ) -> SyscallBaseResult<SecpGetPointFromXResponse> {
        let affine = crate::execution::secp::get_point_from_x(request.x, request.y_parity)?;
        let optional_ec_point_ptr = match affine.map(|ec_point| self.allocate_point(ec_point, vm)) {
            Some(Ok(ptr)) => Some(ptr),
            Some(Err(err)) => return Err(err),
            None => None,
        };
        Ok(SecpOptionalEcPointResponse { optional_ec_point_ptr })
    }

    #[allow(clippy::result_large_err)]
    pub fn secp_get_xy(
        &mut self,
        request: SecpGetXyRequest,
    ) -> SyscallBaseResult<SecpGetXyResponse> {
        let ec_point = self.get_point_by_ptr(request.ec_point_ptr)?;

        Ok(SecpGetXyResponse { x: ec_point.x.into(), y: ec_point.y.into() })
    }

    #[allow(clippy::result_large_err)]
    pub fn secp_new(
        &mut self,
        vm: &mut VirtualMachine,
        request: SecpNewRequest,
    ) -> SyscallBaseResult<SecpNewResponse> {
        let affine = new_affine::<Curve>(request.x, request.y)?;
        let optional_ec_point_ptr = match affine.map(|ec_point| self.allocate_point(ec_point, vm)) {
            Some(Ok(ptr)) => Some(ptr),
            Some(Err(err)) => return Err(err),
            None => None,
        };
        Ok(SecpNewResponse { optional_ec_point_ptr })
    }

    #[allow(clippy::result_large_err)]
    fn allocate_point(
        &mut self,
        ec_point: short_weierstrass::Affine<Curve>,
        vm: &mut VirtualMachine,
    ) -> SyscallBaseResult<Relocatable> {
        self.conditionally_initialize_points_segment_base(vm);
        let points = &mut self.points;
        let id = points.len();
        points.push(ec_point);
        Ok((self.get_initialized_segments_base() + EC_POINT_SEGMENT_SIZE * id)?)
    }

    fn conditionally_initialize_points_segment_base(&mut self, vm: &mut VirtualMachine) {
        if self.points_segment_base.is_none() {
            self.points_segment_base = Some(vm.add_memory_segment());
        }
    }

    fn get_initialized_segments_base(&self) -> Relocatable {
        self.points_segment_base.expect("Segments_base should be initialized at this point.")
    }

    #[allow(clippy::result_large_err)]
    fn get_point_by_ptr(
        &self,
        ec_point_ptr: Relocatable,
    ) -> SyscallBaseResult<&short_weierstrass::Affine<Curve>> {
        let ec_point_id =
            (ec_point_ptr - self.get_initialized_segments_base())? / EC_POINT_SEGMENT_SIZE;
        self.points.get(ec_point_id).ok_or_else(|| SyscallExecutorBaseError::InvalidSyscallInput {
            input: ec_point_id.into(),
            info: "Invalid Secp point ID".to_string(),
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
    #[allow(clippy::result_large_err)]
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
    #[allow(clippy::result_large_err)]
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
    #[allow(clippy::result_large_err)]
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
    #[allow(clippy::result_large_err)]
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
    #[allow(clippy::result_large_err)]
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallBaseResult<SecpGetXyRequest> {
        Ok(SecpGetXyRequest { ec_point_ptr: relocatable_from_ptr(vm, ptr)? })
    }
}

pub type SecpGetXyResponse = EcPointCoordinates;

impl SyscallResponse for SecpGetXyResponse {
    #[allow(clippy::result_large_err)]
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
    #[allow(clippy::result_large_err)]
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
    #[allow(clippy::result_large_err)]
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallBaseResult<SecpNewRequest> {
        let x = SierraU256::from_memory(vm, ptr)?.to_biguint();
        let y = SierraU256::from_memory(vm, ptr)?.to_biguint();
        Ok(SecpNewRequest { x, y })
    }
}

pub type SecpNewResponse = SecpOptionalEcPointResponse;
pub type Secp256r1NewRequest = EcPointCoordinates;
pub type Secp256r1NewResponse = SecpOptionalEcPointResponse;
