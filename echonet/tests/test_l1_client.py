import sys
from pathlib import Path

parent_dir = Path(__file__).parent.parent
sys.path.insert(0, str(parent_dir))

from l1_client import Log, get_logs, get_timestamp_of_block_by_number


def test_get_logs():
    block_number = 20_861_344  # 0x13e51a0

    result = get_logs(
        from_block=block_number,
        to_block=block_number,
    )

    assert len(result) == 1
    log = result[0]

    expected_log = Log(
        address="0xc662c410c0ecf747543f5ba90660f6abebd9c8c4",
        topics=[
            "0xdb80dd488acf86d17c747445b0eabb5d57c541d3bd7b6b87af987858e5066b2b",
            "0x000000000000000000000000023a2aac5d0fa69e3243994672822ba43e34e5c9",
            "0x07c76a71952ce3acd1f953fd2a3fda8564408b821ff367041c89f44526076633",
            "0x02d757788a8d8d6f21d1cd40bce38a8222d70654214e96ff95d8086e684fbee5",
        ],
        data=(
            "0x0000000000000000000000000000000000000000000000000000000000000060"
            "0000000000000000000000000000000000000000000000000000000000195c23"
            "0000000000000000000000000000000000000000000000000000000000000001"
            "0000000000000000000000000000000000000000000000000000000000000003"
            "001e220c4ac08b2f247d45721e08af1b2d8d65b640cea780534c8f20dc6ea981"
            "000000000000000000000000000000000000000000001c468e3281804cca0000"
            "0000000000000000000000000000000000000000000000000000000000000000"
        ),
        block_number=block_number,
        block_hash="0xe090b2c6fbffb35b6e07d5943938384daa59c8c9fefe487d9952ef9894f2483e",
        transaction_hash="0x66c2ef5ae6708ede5e47daaabfc4b54a53c423160ec27eac06524ea3cd939622",
        transaction_index=146,  # 0x92
        log_index=749,  # 0x2ed
        removed=False,
        block_timestamp=1_727_673_743,  # int("0x66fa358f", 16)
    )

    assert log == expected_log


def test_get_timestamp_of_block_by_number():
    block_number = 20_861_344  # 0x13e51a0
    expected_timestamp = 1727673743

    result = get_timestamp_of_block_by_number(block_number)

    assert result == expected_timestamp


def run_all_tests():
    test_get_logs()
    test_get_timestamp_of_block_by_number()


if __name__ == "__main__":
    run_all_tests()
