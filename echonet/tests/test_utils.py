import copy
from l1_client import L1Client


class L1TestUtils:
    """Samples of the same transaction in different representations used throughout the flow."""

    BLOCK_NUMBER = 23911042
    BLOCK_RANGE = [BLOCK_NUMBER - 10, BLOCK_NUMBER + 10]
    NONCE = 0x19B255

    RAW_JSON_LOG = {
        "address": "0xc662c410c0ecf747543f5ba90660f6abebd9c8c4",
        "topics": [
            "0xdb80dd488acf86d17c747445b0eabb5d57c541d3bd7b6b87af987858e5066b2b",  # event_signature
            "0x000000000000000000000000f5b6ee2caeb6769659f6c091d209dfdcaf3f69eb",  # from_address
            "0x0616757a151c21f9be8775098d591c2807316d992bbc3bb1a5c1821630589256",  # to_address
            "0x01b64b1b3b690b43b9b514fb81377518f4039cd3e4f4914d8a6bdf01d679fb19",  # selector
        ],
        "data": "0x0000000000000000000000000000000000000000000000000000000000000060"
        "000000000000000000000000000000000000000000000000000000000019b255"
        "00000000000000000000000000000000000000000000000000001308aba4ade2"
        "0000000000000000000000000000000000000000000000000000000000000005"
        "00000000000000000000000004c46e830bb56ce22735d5d8fc9cb90309317d0f"
        "000000000000000000000000c50a951c4426760ba75c5253985a16196b342168"
        "011bf9dbebdd770c31ff13808c96a1cb2de15a240274dc527e7d809bb2bf38df"
        "0000000000000000000000000000000000000000000000956dfdeac59085edc3"
        "0000000000000000000000000000000000000000000000000000000000000000",
        "blockHash": "0xb33512d13e1a2ff4f3aa6e799a4a2455249be5198760a3f41300a8362d802bf8",
        "blockNumber": "0x16cda82",
        "blockTimestamp": "0x692c23df",
        "transactionHash": "0x726df509fdd23a944f923a6fc18e80cbe7300a54aa34f8e6bd77e9961ca6ce52",
        "transactionIndex": "0x4f",
        "logIndex": "0x7b",
        "removed": False,
    }

    L1_EVENT = L1Client.L1Event(
        contract_address="0x616757a151c21f9be8775098d591c2807316d992bbc3bb1a5c1821630589256",
        entry_point_selector=0x1B64B1B3B690B43B9B514FB81377518F4039CD3E4F4914D8A6BDF01D679FB19,
        calldata=[
            0xF5B6EE2CAEB6769659F6C091D209DFDCAF3F69EB,
            0x04C46E830BB56CE22735D5D8FC9CB90309317D0F,
            0xC50A951C4426760BA75C5253985A16196B342168,
            0x11BF9DBEBDD770C31FF13808C96A1CB2DE15A240274DC527E7D809BB2BF38DF,
            0x956DFDEAC59085EDC3,
            0x0,
        ],
        nonce=NONCE,
        fee=0x1308ABA4ADE2,
        l1_tx_hash="0x726df509fdd23a944f923a6fc18e80cbe7300a54aa34f8e6bd77e9961ca6ce52",
        block_timestamp=1764500447,
        block_number=23911042,
    )

    # L1_HANDLER tx from feeder gateway, expected to match the L1_EVENT.
    FEEDER_TX = {
        "transaction_hash": "0x83c298ad90f4d1b35c0a324fa162a3ab3d3d3a4dcc046f0965bd045083a472",
        "version": "0x0",
        "contract_address": "0x616757a151c21f9be8775098d591c2807316d992bbc3bb1a5c1821630589256",
        "entry_point_selector": "0x1b64b1b3b690b43b9b514fb81377518f4039cd3e4f4914d8a6bdf01d679fb19",
        "nonce": "0x19b255",
        "calldata": [
            "0xf5b6ee2caeb6769659f6c091d209dfdcaf3f69eb",
            "0x4c46e830bb56ce22735d5d8fc9cb90309317d0f",
            "0xc50a951c4426760ba75c5253985a16196b342168",
            "0x11bf9dbebdd770c31ff13808c96a1cb2de15a240274dc527e7d809bb2bf38df",
            "0x956dfdeac59085edc3",
            "0x0",
        ],
        "type": "L1_HANDLER",
    }

    @staticmethod
    def raw_log_with_nonce(nonce: int) -> dict:
        log = copy.deepcopy(L1TestUtils.RAW_JSON_LOG)

        data = log["data"]
        nonce_hex = f"{nonce:064x}"
        log["data"] = data[:66] + nonce_hex + data[130:]

        return log
