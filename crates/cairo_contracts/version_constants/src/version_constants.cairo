#[starknet::contract]
pub mod version_constants {
    use version_constants::interface::IVersionConstants;

    #[storage]
    struct Storage {}

    #[event]
    #[derive(Drop, starknet::Event)]
    pub enum Event {}

    #[constructor]
    pub fn constructor(ref self: ContractState) {}

    #[abi(embed_v0)]
    impl VersionConstantsImpl of IVersionConstants<ContractState> {}
}
