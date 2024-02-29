// TODO(Dori, 3/3/2024): Delete this dummy code.
pub fn dummy() -> u8 {
    7
}

#[cfg(test)]
pub mod test {
    use super::dummy;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_dummy() {
        assert_eq!(dummy(), 7);
    }
}
