use cairo_native::starknet::U256;

use super::Secp256Point;

#[test]
fn infinity_test() {
    let p1 = Secp256Point::<ark_secp256k1::Config>::get_point_from_x(U256 { lo: 1, hi: 0 }, false)
        .unwrap()
        .unwrap();

    let p2 = Secp256Point::mul(p1, U256 { lo: 0, hi: 0 });
    assert!(p2.0.infinity);

    assert_eq!(p1, Secp256Point::add(p1, p2));
}
