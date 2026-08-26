import csv
import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "../.."))

from echonet.fee_recorder import (
    CSV_COLUMNS,
    MAX_CALLED_CONTRACTS,
    FeeRecorder,
    FeeRecorderError,
    called_contracts_from_calldata,
    create_fee_recorder,
    normalize_address,
)


def build_call(target: str, selector: str, inner_calldata: list[str]) -> list[str]:
    """One call under the Cairo 1 layout, which carries its arguments inline."""
    return [target, selector, hex(len(inner_calldata)), *inner_calldata]


def build_split_calldata_multicall(calls: list[tuple[str, list[str]]]) -> list[str]:
    """A whole Cairo 0 `__execute__` buffer, whose calls index into one shared argument blob."""
    call_array: list[str] = []
    shared_calldata: list[str] = []
    for target, inner_calldata in calls:
        call_array.extend([target, "0x1", hex(len(shared_calldata)), hex(len(inner_calldata))])
        shared_calldata.extend(inner_calldata)
    return [hex(len(calls)), *call_array, hex(len(shared_calldata)), *shared_calldata]


def build_block_document(
    block_number: int = 100, transactions=None, receipts=None
) -> dict[str, object]:
    price = {"price_in_fri": "1000", "price_in_wei": "500"}
    return {
        "block_number": block_number,
        "starknet_version": "0.14.4",
        "l1_gas_price": price,
        "l1_data_gas_price": price,
        "l2_gas_price": {"price_in_fri": "27400000000", "price_in_wei": "7"},
        "transactions": transactions if transactions is not None else [],
        "transaction_receipts": receipts if receipts is not None else [],
    }


def build_receipt(
    actual_fee: str = "1234",
    execution_status: str = "SUCCEEDED",
    l2_gas: int = 5_000_000,
    revert_error: str | None = None,
) -> dict[str, object]:
    receipt: dict[str, object] = {
        "execution_status": execution_status,
        "actual_fee": actual_fee,
        "execution_resources": {
            "n_steps": 4321,
            "total_gas_consumed": {"l1_gas": 0, "l1_data_gas": 128, "l2_gas": l2_gas},
        },
    }
    if revert_error is not None:
        receipt["revert_error"] = revert_error
    return receipt


class TestCalledContractsFromCalldata(unittest.TestCase):
    def test_single_call(self):
        calldata = ["0x1", *build_call("0x0abc", "0xdead", ["0x1", "0x2"])]
        self.assertEqual(called_contracts_from_calldata(calldata), ["0xabc"])

    def test_multicall_preserves_order(self):
        calldata = [
            "0x2",
            *build_call("0xaa", "0x1", []),
            *build_call("0xbb", "0x2", ["0x9"]),
        ]
        self.assertEqual(called_contracts_from_calldata(calldata), ["0xaa", "0xbb"])

    def test_empty_calldata(self):
        self.assertEqual(called_contracts_from_calldata([]), [])

    def test_zero_calls(self):
        self.assertEqual(called_contracts_from_calldata(["0x0"]), [])

    def test_trailing_felts_reject_the_whole_parse(self):
        calldata = ["0x1", *build_call("0xaa", "0x1", []), "0x99"]
        self.assertEqual(called_contracts_from_calldata(calldata), [])

    def test_truncated_call_rejects_the_whole_parse(self):
        self.assertEqual(called_contracts_from_calldata(["0x1", "0xaa", "0x1"]), [])

    def test_inner_length_past_the_buffer_rejects_the_whole_parse(self):
        calldata = ["0x1", "0xaa", "0x1", hex(2**64)]
        self.assertEqual(called_contracts_from_calldata(calldata), [])

    def test_call_count_past_the_buffer_rejects_the_whole_parse(self):
        self.assertEqual(called_contracts_from_calldata([hex(2**200), "0xaa"]), [])

    def test_non_hex_calldata_rejects_the_whole_parse(self):
        self.assertEqual(called_contracts_from_calldata(["1", "2", "3"]), [])

    def test_split_calldata_layout(self):
        calldata = build_split_calldata_multicall([("0xaa", ["0x7", "0x8"]), ("0xbb", ["0x9"])])
        self.assertEqual(called_contracts_from_calldata(calldata), ["0xaa", "0xbb"])

    def test_split_calldata_layout_with_no_arguments(self):
        calldata = build_split_calldata_multicall([("0xaa", [])])
        self.assertEqual(called_contracts_from_calldata(calldata), ["0xaa"])

    def test_split_calldata_layout_with_a_wrong_length_rejects_the_whole_parse(self):
        calldata = build_split_calldata_multicall([("0xaa", ["0x7"])])
        self.assertEqual(called_contracts_from_calldata([*calldata, "0x0"]), [])

    def test_an_address_only_in_a_call_argument_is_not_reported(self):
        recipient = "0xdeadbeef"
        calldata = ["0x1", *build_call("0xaa", "0x1", [recipient, "0x5"])]
        self.assertEqual(called_contracts_from_calldata(calldata), ["0xaa"])

    def test_targets_are_capped(self):
        n_calls = MAX_CALLED_CONTRACTS + 3
        calldata = [hex(n_calls)]
        for call_index in range(n_calls):
            calldata.extend(build_call(hex(call_index + 1), "0x1", []))
        self.assertEqual(len(called_contracts_from_calldata(calldata)), MAX_CALLED_CONTRACTS)


class TestNormalizeAddress(unittest.TestCase):
    def test_zero_padding_is_dropped(self):
        self.assertEqual(
            normalize_address("0x0004718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c93"),
            "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c93",
        )

    def test_case_is_normalized(self):
        self.assertEqual(normalize_address("0xABC"), "0xabc")

    def test_non_hex_is_passed_through(self):
        self.assertEqual(normalize_address(""), "")
        self.assertEqual(normalize_address("0xnope"), "0xnope")


class TestFeeRecorder(unittest.TestCase):
    def test_rejects_a_label_that_could_escape_the_directory(self):
        with tempfile.TemporaryDirectory() as log_dir:
            for label in ("../escape", "a/b", "", "-leading"):
                with self.assertRaises(FeeRecorderError):
                    FeeRecorder(run_label=label, log_dir=Path(log_dir))

    def test_create_returns_none_without_a_label(self):
        with tempfile.TemporaryDirectory() as log_dir:
            self.assertIsNone(create_fee_recorder(run_label="", log_dir=Path(log_dir)))

    def test_records_echonet_and_mainnet_columns(self):
        transaction = {
            "transaction_hash": "0xf1",
            "type": "INVOKE",
            "sender_address": "0x00AA",
            "tip": "0x3b9aca00",
            "calldata": ["0x1", *build_call("0xbeef", "0x1", [])],
        }
        block_document = build_block_document(
            transactions=[transaction],
            receipts=[build_receipt(actual_fee="2000", l2_gas=6_000_000)],
        )
        with tempfile.TemporaryDirectory() as log_dir:
            recorder = FeeRecorder(run_label="v14_4", log_dir=Path(log_dir))
            recorder.record_block(
                block_document=block_document,
                source_block_number_by_tx_hash={"0xf1": 42},
                mainnet_receipt_by_tx_hash={
                    "0xf1": build_receipt(actual_fee="1900", l2_gas=5_900_000)
                },
            )
            rows = list(csv.DictReader(recorder.csv_path.open()))

        self.assertEqual(len(rows), 1)
        row = rows[0]
        self.assertEqual(list(row.keys()), list(CSV_COLUMNS))
        self.assertEqual(row["run_label"], "v14_4")
        self.assertEqual(row["starknet_version"], "0.14.4")
        self.assertEqual(row["block_number"], "100")
        self.assertEqual(row["source_block_number"], "42")
        self.assertEqual(row["sender_address"], "0xaa")
        self.assertEqual(row["called_contracts"], "0xbeef")
        self.assertEqual(row["actual_fee"], "2000")
        self.assertEqual(row["l2_gas"], "6000000")
        self.assertEqual(row["mainnet_actual_fee"], "1900")
        self.assertEqual(row["mainnet_l2_gas"], "5900000")
        self.assertEqual(row["l2_gas_price_fri"], "27400000000")
        self.assertEqual(row["tip_fri_per_l2_gas"], "0x3b9aca00")
        self.assertEqual(row["revert_error"], "")

    def test_records_a_transaction_mainnet_has_no_receipt_for(self):
        block_document = build_block_document(
            transactions=[{"transaction_hash": "0xf2", "type": "L1_HANDLER"}],
            receipts=[build_receipt(actual_fee="7")],
        )
        with tempfile.TemporaryDirectory() as log_dir:
            recorder = FeeRecorder(run_label="v14_3", log_dir=Path(log_dir))
            recorder.record_block(
                block_document=block_document,
                source_block_number_by_tx_hash={},
                mainnet_receipt_by_tx_hash={},
            )
            rows = list(csv.DictReader(recorder.csv_path.open()))

        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0]["source_block_number"], "")
        self.assertEqual(rows[0]["mainnet_actual_fee"], "")
        self.assertEqual(rows[0]["tip_fri_per_l2_gas"], "")
        self.assertEqual(rows[0]["actual_fee"], "7")

    def test_revert_error_is_flattened_and_truncated(self):
        block_document = build_block_document(
            transactions=[{"transaction_hash": "0xf3", "type": "INVOKE"}],
            receipts=[
                build_receipt(
                    execution_status="REVERTED", revert_error="line one\nline  two " + "x" * 500
                )
            ],
        )
        with tempfile.TemporaryDirectory() as log_dir:
            recorder = FeeRecorder(run_label="v14_4", log_dir=Path(log_dir))
            recorder.record_block(
                block_document=block_document,
                source_block_number_by_tx_hash={},
                mainnet_receipt_by_tx_hash={},
            )
            rows = list(csv.DictReader(recorder.csv_path.open()))

        revert_error = rows[0]["revert_error"]
        self.assertEqual(len(revert_error), 300)
        self.assertTrue(revert_error.startswith("line one line two x"))

    def test_header_is_written_once_across_recorders(self):
        block_document = build_block_document(
            transactions=[{"transaction_hash": "0xf4", "type": "INVOKE"}],
            receipts=[build_receipt()],
        )
        with tempfile.TemporaryDirectory() as log_dir:
            for _restart in range(2):
                recorder = FeeRecorder(run_label="v14_4", log_dir=Path(log_dir))
                recorder.record_block(
                    block_document=block_document,
                    source_block_number_by_tx_hash={},
                    mainnet_receipt_by_tx_hash={},
                )
            lines = recorder.csv_path.read_text().splitlines()

        self.assertEqual(len(lines), 3)
        self.assertEqual(lines[0], ",".join(CSV_COLUMNS))

    def test_empty_block_writes_nothing(self):
        with tempfile.TemporaryDirectory() as log_dir:
            recorder = FeeRecorder(run_label="v14_4", log_dir=Path(log_dir))
            recorder.record_block(
                block_document=build_block_document(),
                source_block_number_by_tx_hash={},
                mainnet_receipt_by_tx_hash={},
            )
            self.assertEqual(recorder.csv_path.read_text().splitlines(), [",".join(CSV_COLUMNS)])


if __name__ == "__main__":
    unittest.main()
