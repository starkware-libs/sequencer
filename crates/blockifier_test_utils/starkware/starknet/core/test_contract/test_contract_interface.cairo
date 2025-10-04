%lang starknet

struct StorageCell {
    key: felt,
    value: felt,
}

@contract_interface
namespace TestContract {
    func set_value(address: felt, value: felt) {
    }

    func get_value(address: felt) -> (res: felt) {
    }

    func add_value(value: felt) {
    }

    func recursive_add_value(self_address: felt, value: felt) {
    }

    func test_call_with_array(self_address, arr_len: felt, arr: felt*) {
    }

    func test_call_with_struct_array(self_address, arr_len: felt, arr: StorageCell*) {
    }
}
