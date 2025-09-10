use starknet_types_core::felt::Felt;

use crate::hints::hint_implementation::state_diff_encryption::ec::{
    ec_add,
    ec_double,
    ec_mult,
    ec_safe_add,
    ec_safe_mult,
    recover_y,
    y_squared_from_x,
    EcPoint,
    EcPointOrInfinity,
};

fn find_point(alpha: Felt, beta: Felt, start: u64, require_nonzero_y: bool) -> EcPoint {
    let mut x = Felt::from(start as u64);
    loop {
        if let Ok(y) = recover_y(x, alpha, beta) {
            if !require_nonzero_y || y != Felt::ZERO {
                return (x, y);
            }
        }
        x = x + Felt::ONE;
    }
}

#[test]
fn test_ec_add_distinct_x() {
    // Curve: y^2 = x^3 + alpha*x + beta.
    let alpha = Felt::from(1u8);
    let beta = Felt::from(1u8);
    let p1: EcPoint = find_point(alpha, beta, 1, false);
    let mut p2: EcPoint = find_point(alpha, beta, 2, false);
    if p2.0 == p1.0 { p2 = find_point(alpha, beta, 3, false); }
    let sum = ec_add(p1, p2);
    assert_eq!(sum.1 * sum.1, y_squared_from_x(sum.0, alpha, beta));
}

#[test]
fn test_ec_add_vertical_line_returns_infinity() {
    let alpha = Felt::from(1u8);
    let beta = Felt::from(1u8);
    let p1: EcPoint = find_point(alpha, beta, 5, false);
    let p2: EcPoint = (p1.0, Felt::ZERO - p1.1);
    let res = ec_safe_add(EcPointOrInfinity::Point(p1), EcPointOrInfinity::Point(p2), alpha);
    assert!(matches!(res, EcPointOrInfinity::Infinity));
}

#[test]
fn test_ec_double() {
    let alpha = Felt::from(1u8);
    let beta = Felt::from(1u8);
    let p: EcPoint = find_point(alpha, beta, 10, true);
    let doubled_via_double = ec_double(p, alpha);
    let doubled_via_safe_add = match ec_safe_add(EcPointOrInfinity::Point(p), EcPointOrInfinity::Point(p), alpha) {
        EcPointOrInfinity::Point(q) => q,
        EcPointOrInfinity::Infinity => panic!("unexpected infinity for y!=0"),
    };
    assert_eq!(doubled_via_double, doubled_via_safe_add);
}

#[test]
#[should_panic]
fn test_ec_double_y_zero_panics() {
    let alpha = Felt::from(1u8);
    let _ = ec_double((Felt::ONE, Felt::ZERO), alpha);
}

#[test]
fn test_ec_mult_basic_relations() {
    let alpha = Felt::from(2u8);
    let beta = Felt::from(1u8);
    let p: EcPoint = find_point(alpha, beta, 20, true);
    // 2*P via safe mult equals safe add of P+P.
    let two_p_safe = match ec_safe_mult(2, EcPointOrInfinity::Point(p), alpha) {
        EcPointOrInfinity::Point(q) => q,
        EcPointOrInfinity::Infinity => panic!("unexpected infinity for y!=0"),
    };
    let p_plus_p = match ec_safe_add(EcPointOrInfinity::Point(p), EcPointOrInfinity::Point(p), alpha) {
        EcPointOrInfinity::Point(q) => q,
        EcPointOrInfinity::Infinity => panic!("unexpected infinity for y!=0"),
    };
    assert_eq!(two_p_safe, p_plus_p);

    // 3*P via safe mult equals (2P)+P via safe add.
    let three_p_safe = match ec_safe_mult(3, EcPointOrInfinity::Point(p), alpha) {
        EcPointOrInfinity::Point(q) => q,
        EcPointOrInfinity::Infinity => panic!("unexpected infinity for this test"),
    };
    let two_p_plus_p = match ec_safe_add(EcPointOrInfinity::Point(two_p_safe), EcPointOrInfinity::Point(p), alpha) {
        EcPointOrInfinity::Point(q) => q,
        EcPointOrInfinity::Infinity => panic!("unexpected infinity for this test"),
    };
    assert_eq!(three_p_safe, two_p_plus_p);
}

#[test]
fn test_y_squared_and_recover_y() {
    let alpha = Felt::from(1u8);
    let beta = Felt::from(1u8);
    let p: EcPoint = find_point(alpha, beta, 30, false);
    let y_sq = y_squared_from_x(p.0, alpha, beta);
    let y = recover_y(p.0, alpha, beta).expect("point should be on curve");
    assert_eq!(y * y, y_sq);
}

#[test]
fn test_safe_add_with_infinity() {
    let alpha = Felt::from(1u8);
    let beta = Felt::from(1u8);
    let a: EcPoint = find_point(alpha, beta, 40, false);
    let inf = EcPointOrInfinity::Infinity;
    let res = ec_safe_add(inf, EcPointOrInfinity::Point(a), alpha);
    assert!(matches!(res, EcPointOrInfinity::Point(p) if p == a));
}
