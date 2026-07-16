import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import MagicMock, patch

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "../.."))

from echonet import os_runner_worker


def _write_payload(input_path: Path, block_number) -> None:
    input_path.write_text(
        json.dumps(
            {
                "blob": {},
                "state_commitment_infos": {},
                "block_document": {"parent_block_hash": "0x0", "block_hash": "0x1"},
                "block_number": block_number,
                "block_hash_commitments_payload": {},
            }
        ),
        encoding="utf-8",
    )


def _fake_config(log_dir: Path, cli_path: Path) -> MagicMock:
    config = MagicMock()
    config.paths.block_hash_cli_path = cli_path
    config.paths.log_dir = log_dir
    config.os_runner.cli_timeout_secs = 5
    config.os_runner.chain_id = "SN_MAIN"
    config.os_runner.strk_fee_token_address = "0x0"
    config.os_runner.layout = "all_cairo"
    config.os_runner.max_failed_dumps = 10
    return config


class TestOsRunnerWorkerBlockNumberValidation(unittest.TestCase):
    """
    `block_number` from the `--input-path` payload is interpolated into a
    failed-run dump directory name (`dump_root / f"block_{block_number}_failed"`).
    Without validation, a malformed value such as "../../../tmp/pwned" escapes
    the intended `os_runs` directory (pathlib splits on "/" when joining), letting
    an attacker-influenced payload create directories and write files anywhere
    reachable by a relative walk from `os_runs`.
    """

    def test_path_traversal_block_number_is_rejected_before_any_side_effect(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            log_dir = tmp_path / "data" / "echonet"
            log_dir.mkdir(parents=True)
            cli_path = tmp_path / "fake_cli"
            cli_path.touch()
            input_path = tmp_path / "input.json"
            _write_payload(input_path, "../../../../tmp/echonet_poc_pwned")

            fake_config = _fake_config(log_dir, cli_path)
            # With no validation, `dump_root / f"block_{block_number}_failed"` resolves
            # (verified against the pre-fix code) to log_dir/tmp/echonet_poc_pwned_failed —
            # a sibling of log_dir/echonet, i.e. outside the intended os_runs sandbox.
            traversal_target = log_dir.parent / "tmp" / "echonet_poc_pwned_failed"

            def fake_subprocess_run(cmd, **kwargs):
                # If reached, the CLI subprocess "fails", driving main() into the
                # failed-run dump branch that builds the path from block_number.
                raise os_runner_worker.subprocess.CalledProcessError(
                    1, cmd, output="", stderr="boom"
                )

            with patch.object(os_runner_worker, "CONFIG", fake_config), patch.object(
                sys, "argv", ["os_runner_worker.py", "--input-path", str(input_path)]
            ), patch.object(
                os_runner_worker, "build_os_cli_input", return_value={"os_hints": {"os_input": {}}}
            ) as mock_build_input, patch.object(
                os_runner_worker, "resolve_classes_for_os", return_value=({}, {}, 0, 0)
            ), patch.object(
                os_runner_worker.subprocess, "run", side_effect=fake_subprocess_run
            ):
                with self.assertRaises(ValueError):
                    os_runner_worker.main()

            # The traversal payload must never reach input assembly, the CLI subprocess,
            # or the failed-run dump: validation must happen up front, not rely on
            # downstream code paths to keep the write inside `os_runs`.
            mock_build_input.assert_not_called()
            self.assertFalse(traversal_target.exists(), "block_number escaped the dump sandbox")
            self.assertFalse((log_dir / "os_runs").exists())

    def test_valid_block_number_still_succeeds(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            log_dir = tmp_path / "data" / "echonet"
            log_dir.mkdir(parents=True)
            cli_path = tmp_path / "fake_cli"
            cli_path.touch()
            input_path = tmp_path / "input.json"
            _write_payload(input_path, "12345")

            fake_config = _fake_config(log_dir, cli_path)

            def fake_subprocess_run(cmd, **kwargs):
                output_path = Path(cmd[cmd.index("--output-path") + 1])
                output_path.write_text(json.dumps({"da_segment": [], "unused_hints": []}))
                return MagicMock(returncode=0)

            with patch.object(os_runner_worker, "CONFIG", fake_config), patch.object(
                sys, "argv", ["os_runner_worker.py", "--input-path", str(input_path)]
            ), patch.object(
                os_runner_worker, "build_os_cli_input", return_value={"os_hints": {"os_input": {}}}
            ) as mock_build_input, patch.object(
                os_runner_worker, "resolve_classes_for_os", return_value=({}, {}, 0, 0)
            ), patch.object(
                os_runner_worker.subprocess, "run", side_effect=fake_subprocess_run
            ):
                exit_code = os_runner_worker.main()

            self.assertEqual(exit_code, 0)
            self.assertEqual(mock_build_input.call_args.kwargs["block_number"], 12345)
            self.assertIsInstance(mock_build_input.call_args.kwargs["block_number"], int)


if __name__ == "__main__":
    unittest.main()
