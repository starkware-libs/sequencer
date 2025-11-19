from dataclasses import dataclass
from typing import List

import logging
import requests

logger = logging.getLogger(__name__)


@dataclass(frozen=True)
class Log:
    """
    Ethereum log entry
    """

    address: str
    topics: List[str]
    data: str
    block_number: int
    block_hash: str
    transaction_hash: str
    transaction_index: int
    log_index: int
    removed: bool
    block_timestamp: int


class L1Client:
    ALCHEMY_URL = "https://eth-mainnet.g.alchemy.com/v2/your-api-key"

    @staticmethod
    def get_logs(from_block: int, to_block: int, rpc_url: str = ALCHEMY_URL) -> List[Log]:
        """
        Get logs from Ethereum using eth_getLogs RPC method.
        Tries up to 2 times. On failure, logs an error and returns [].
        """
        if from_block > to_block:
            raise ValueError("from_block must be less than or equal to to_block")

        payload = {
            "jsonrpc": "2.0",
            "method": "eth_getLogs",
            "params": [
                {
                    "fromBlock": hex(from_block),
                    "toBlock": hex(to_block),
                    "address": "0xc662c410C0ECf747543f5bA90660f6ABeBD9C8c4",  # Starknet L1 contract
                    "topics": [
                        "0xdb80dd488acf86d17c747445b0eabb5d57c541d3bd7b6b87af987858e5066b2b"
                        # LogMessageToL2 event signature
                    ],
                }
            ],
            "id": 1,
        }

        last_exc = None
        for _ in range(2):
            try:
                response = requests.post(rpc_url, json=payload, timeout=10)
                response.raise_for_status()
                data = response.json()
                break  # success -> exit loop
            except (requests.RequestException, ValueError) as exc:
                last_exc = exc
        else:
            logger.error(
                "get_logs failed after 2 attempts, returning empty list",
                extra={"url": rpc_url, "from_block": from_block, "to_block": to_block},
                exc_info=last_exc,
            )
            return []

        results = data.get("result", [])

        return [
            Log(
                address=result["address"],
                topics=result["topics"],
                data=result["data"],
                block_number=int(result["blockNumber"], 16),
                block_hash=result["blockHash"],
                transaction_hash=result["transactionHash"],
                transaction_index=int(result["transactionIndex"], 16),
                log_index=int(result["logIndex"], 16),
                removed=result["removed"],
                block_timestamp=int(result["blockTimestamp"], 16),
            )
            for result in results
        ]
