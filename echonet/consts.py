from pathlib import Path

# Shared throttling headers used for feeder requests
FEEDER_HEADERS = {"X-Throttling-Bypass": "QYHGVPY7PHER3QHI6LWBY25AGF5GGEZ"}

# Shared base URLs
FEEDER_BASE_URL = "https://feeder.alpha-mainnet.starknet.io"
SEQUENCER_BASE_URL_DEFAULT = "http://sequencer-node-service:8080"

# Shared default starting block number (used by multiple apps)
START_BLOCK_DEFAULT = 3486720
END_BLOCK_DEFAULT = None
SLEEP_BETWEEN_BLOCKS_SECONDS_DEFAULT = 2.0


# Feeder endpoints
GET_BLOCK_ENDPOINT = "/feeder_gateway/get_block"
GET_STATE_UPDATE_ENDPOINT = "/feeder_gateway/get_state_update"
GET_SIGNATURE_ENDPOINT = "/feeder_gateway/get_signature"
GET_TRANSACTION_ENDPOINT = "/feeder_gateway/get_transaction"
GET_CLASS_BY_HASH_ENDPOINT = "/feeder_gateway/get_class_by_hash"

# Sequencer endpoints
ADD_TX_ENDPOINT = "/gateway/add_transaction"

# Shared log directory for auxiliary files (not block storage)
LOG_DIR = Path("/data/echonet")

# Common file paths under /data/echonet
TX_SENDER_CONTROL_FILE = str(LOG_DIR / "tx_sender.cmd")
TX_SENDER_STATUS_FILE = str(LOG_DIR / "tx_sender.status")

#
# Domain constants shared by apps
#
CUSTOM_FIELDS = {
    "state_root": "0x6138090a2ceae6c179b01b9bbdc13c74d03063cf3f801017cc5fc6bae514881",
    "transaction_commitment": "0x1432ac404fda8b7c921df9c55f1b1539e3f982837a62f6b9db6c843de9f2e85",
    "event_commitment": "0x76747e447201cbd15d044fffff15d30aca29681cef8442ee771cd90692b4b2e",
    "receipt_commitment": "0x107308016a4910c9ad57fc4cf7ce2fc2bfafddd6ca6def98bbfdbeea16429d2",
    "state_diff_length": 159,
    "status": "ACCEPTED_ON_L1",
    "l1_da_mode": "BLOB",
    "l2_gas_consumed": 606837191,
    "next_l2_gas_price": "0xb2d05e0",
    "sequencer_address": "0x1176a1bd84444c89232ec27754698e5d2e7e1a7f1539f12027f28b23ec9f3d8",
    "starknet_version": "0.14.1",
}

SIGNATURE_CONST = {
    "block_hash": "0x8470dbc0e524e713c926511c3b1b5c8512b083f925bf0bd247f0a46ed91a4a",
    "signature": [
        "0x5447b6b452f704af805b2166f863c3b31a0b864f56ed2e5adce5fe64fec162e",
        "0x61dee68ee8a9ade268bc6b3515484350d29dd6ffc5804ff300eda40460575ef",
    ],
}

# Static list of blocked sender addresses for transaction filtering.
# Update this set to add/remove blocked senders. Values are case-insensitive.
BLOCKED_SENDERS = set()
