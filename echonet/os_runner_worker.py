"""
Standalone CLI tool that runs the Starknet OS for one block.

echo_center spawns this as a subprocess per OS run. The process loads CONFIG
(from /data/echonet/echonet_keys.json), builds the `OsCliInput` JSON, invokes
`committer-and-os-cli os run-os-stateless`, then exits — so all retained
Python heap (captured CLI output, intermediate JSON dicts, cairo-lang imports
for component-hash derivation) dies with this process. echo_center stays clean.

Protocol:
  argv:   --input-path <path>     # JSON file written by echo_center; see below
  stdin:  unused (DEVNULL)
  stdout: empty on success (any output is treated as log noise by echo_center)
  stderr: human-readable log lines, inherited to echo_center's stderr
  exit:   0 = success, non-zero = failure. On failure the failing OS CLI
          `input.json` is dumped to /data/echonet/os_runs/block_<N>_failed/
          (rolling retention of the most recent _MAX_FAILED_DUMPS dirs).

Input file shape:
  {
    "blob":                          AerospikeBlob (from cende write_blob),
    "state_commitment_infos":        StateCommitmentInfos for block_number,
    "block_document":                {parent_block_hash, block_hash},
    "block_number":                  int,
    "block_hash_commitments_payload": JSON from block-hash CLI (computed in
                                      echo_center; small payload — txs +
                                      thin state diff, not the full blob)
  }
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from echonet.class_fetcher import resolve_classes_for_os
from echonet.echonet_types import CONFIG
from echonet.os_input_builder import build_os_cli_input

_MAX_FAILED_DUMPS = 10
_CLASS_CACHE_SUBDIR = "class_cache"


def _prune_old_failed_dumps(dump_root: Path, *, keep: int) -> None:
    """Keep at most `keep` most-recent `block_<N>_failed` directories; delete the rest."""
    try:
        entries = [
            child
            for child in dump_root.iterdir()
            if child.is_dir() and child.name.startswith("block_") and child.name.endswith("_failed")
        ]
    except OSError:
        return
    if len(entries) <= keep:
        return
    entries.sort(key=lambda p: p.stat().st_mtime, reverse=True)
    for stale in entries[keep:]:
        shutil.rmtree(stale, ignore_errors=True)


def _log(msg: str) -> None:
    sys.stderr.write(msg + "\n")
    sys.stderr.flush()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input-path", required=True, type=Path)
    args = parser.parse_args()
    with args.input_path.open("r", encoding="utf-8") as fp:
        payload = json.load(fp)
    blob = payload["blob"]
    state_commitment_infos = payload["state_commitment_infos"]
    block_document = payload["block_document"]
    block_number = int(payload["block_number"])
    block_hash_commitments_payload = payload["block_hash_commitments_payload"]

    cli_path = CONFIG.paths.block_hash_cli_path
    if not cli_path.exists():
        _log(f"Missing committer-and-os-cli binary at: {cli_path}")
        return 2
    timeout = CONFIG.os_runner.cli_timeout_secs
    repo_root = Path(__file__).resolve().parent.parent

    with tempfile.TemporaryDirectory(prefix="echonet_os_", dir=repo_root) as tmp:
        tmp_path = Path(tmp)
        input_path = tmp_path / "input.json"
        output_path = tmp_path / "output.json"
        cairo_pie_zip_path = tmp_path / "pie.zip"
        raw_os_output_path = tmp_path / "raw_os_output.json"
        os_cli_input = build_os_cli_input(
            blob,
            state_commitment_infos=state_commitment_infos,
            block_number=block_number,
            prev_block_hash=block_document["parent_block_hash"],
            new_block_hash=block_document["block_hash"],
            block_hash_commitments_payload=block_hash_commitments_payload,
            chain_id=CONFIG.os_runner.chain_id,
            strk_fee_token_address=CONFIG.os_runner.strk_fee_token_address,
            layout=CONFIG.os_runner.layout,
            cairo_pie_zip_path=str(cairo_pie_zip_path),
            raw_os_output_path=str(raw_os_output_path),
        )
        cache_root = CONFIG.paths.log_dir / _CLASS_CACHE_SUBDIR
        (
            compiled_classes,
            deprecated_compiled_classes,
            fetched_count,
            cached_count,
        ) = resolve_classes_for_os(blob, cache_root=cache_root)
        os_input = os_cli_input["os_hints"]["os_input"]
        os_input["compiled_classes"] = compiled_classes
        os_input["deprecated_compiled_classes"] = deprecated_compiled_classes
        _log(
            f"block {block_number} classes: "
            f"compiled={len(compiled_classes)} deprecated={len(deprecated_compiled_classes)} "
            f"fetched={fetched_count} cached={cached_count}"
        )
        input_path.write_text(json.dumps(os_cli_input), encoding="utf-8")
        try:
            subprocess.run(
                [
                    str(cli_path),
                    "os",
                    "run-os-stateless",
                    "--input-path",
                    str(input_path),
                    "--output-path",
                    str(output_path),
                ],
                cwd=repo_root,
                capture_output=True,
                text=True,
                check=True,
                timeout=timeout,
            )
        except subprocess.CalledProcessError as exc:
            dump_root = CONFIG.paths.log_dir / "os_runs"
            dump_dir = dump_root / f"block_{block_number}_failed"
            try:
                dump_dir.mkdir(parents=True, exist_ok=True)
                (dump_dir / "input.json").write_bytes(input_path.read_bytes())
                _prune_old_failed_dumps(dump_root, keep=_MAX_FAILED_DUMPS)
            except Exception as dump_err:
                _log(f"Failed to dump input.json: {dump_err}")
            _log(
                f"OS CLI run failed for block {block_number} (exit {exc.returncode}); "
                f"input dumped at {dump_dir}/input.json\n"
                f"stderr (last 2KB): {exc.stderr[-2048:]}"
            )
            return 1

        result = json.loads(output_path.read_text(encoding="utf-8"))
        da_segment = result.get("da_segment") or []
        unused_hints = result.get("unused_hints") or []
        json.dump(
            {
                "da_segment_len": len(da_segment),
                "unused_hints_count": len(unused_hints),
            },
            sys.stdout,
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
