from typing import Optional

from l1_client import L1Client
from l1_events import L1Events


def find_l1_block_for_tx(
    feeder_tx: dict,
    l2_block_timestamp: int,
    client: L1Client,
    search_minutes_before: int = 5,
) -> Optional[int]:
    """
    Finds the L1 block number that contains the given L1 handler transaction.
    """
    search_start_timestamp = l2_block_timestamp - (search_minutes_before * 60)
    search_end_timestamp = l2_block_timestamp

    start_block_data = client.get_block_number_by_timestamp(search_start_timestamp)
    end_block_data = client.get_block_number_by_timestamp(search_end_timestamp)

    if not start_block_data or not end_block_data:
        return None

    logs = client.get_logs(start_block_data, end_block_data)

    for log in logs:
        l1_event = L1Events.parse_event(log)

        if L1Events.l1_event_matches_feeder_tx(l1_event, feeder_tx):
            return log.block_number

    # Not found in this range
    return None
