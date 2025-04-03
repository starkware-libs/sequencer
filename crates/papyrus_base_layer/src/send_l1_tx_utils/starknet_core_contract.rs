// TODO(Arni): Use Alloy instead of ethers. Use
// [crate::ethereum_base_layer_contract::StarknetL1Contract].

// Generate contract bindings for the function
ethers::contract::abigen!(
    L1Messenger,
    r#"
        [
            {
                "inputs": [
                    { "internalType": "uint256", "name": "toAddress", "type": "uint256" },
                    { "internalType": "uint256", "name": "selector", "type": "uint256" },
                    { "internalType": "uint256[]", "name": "payload", "type": "uint256[]" }
                ],
                "name": "sendMessageToL2",
                "outputs": [
                    { "internalType": "bytes32", "name": "", "type": "bytes32" },
                    { "internalType": "uint256", "name": "", "type": "uint256" }
                ],
                "stateMutability": "payable",
                "type": "function"
            }
        ]
    "#
);
