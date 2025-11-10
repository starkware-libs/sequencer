import json
import os
import pathlib
from typing import Any, Dict, Iterable, List, Optional, Tuple

from bisect import bisect_left, insort


class BlockStorage:
    def __init__(self, root_dir: str):
        self.root = pathlib.Path(root_dir)
        self.root.mkdir(parents=True, exist_ok=True)
        self._numbers_sorted: Optional[List[int]] = None

    def _ensure_index_loaded(self) -> None:
        if self._numbers_sorted is None:
            nums: List[int] = []
            for n in self.iter_block_numbers():
                nums.append(n)
            nums.sort()
            self._numbers_sorted = nums

    def _insert_number(self, block_number: int) -> None:
        self._ensure_index_loaded()
        assert self._numbers_sorted is not None
        i = bisect_left(self._numbers_sorted, block_number)
        if i == len(self._numbers_sorted) or self._numbers_sorted[i] != block_number:
            insort(self._numbers_sorted, block_number)

    def iter_block_numbers(self) -> Iterable[int]:
        for p in self.root.glob("*.json"):
            try:
                yield int(p.stem)
            except Exception:
                continue

    def get_sorted_block_numbers(self) -> List[int]:
        self._ensure_index_loaded()
        assert self._numbers_sorted is not None
        return list(self._numbers_sorted)

    def get_highest_block_number(self) -> Optional[int]:
        self._ensure_index_loaded()
        assert self._numbers_sorted is not None
        if not self._numbers_sorted:
            return None
        return self._numbers_sorted[-1]

    def get_lowest_block_number(self) -> Optional[int]:
        self._ensure_index_loaded()
        assert self._numbers_sorted is not None
        if not self._numbers_sorted:
            return None
        return self._numbers_sorted[0]

    def contains_block(self, block_number: int) -> bool:
        self._ensure_index_loaded()
        assert self._numbers_sorted is not None
        i = bisect_left(self._numbers_sorted, block_number)
        return i < len(self._numbers_sorted) and self._numbers_sorted[i] == block_number

    def get_index(self, block_number: int) -> Optional[int]:
        """Return zero-based index of block_number among stored blocks when ordered ascending.
        Returns None if block_number not present.
        """
        self._ensure_index_loaded()
        assert self._numbers_sorted is not None
        i = bisect_left(self._numbers_sorted, block_number)
        if i < len(self._numbers_sorted) and self._numbers_sorted[i] == block_number:
            return i
        return None

    def read_block(self, block_number: int) -> Optional[Dict[str, Any]]:
        path = self.root / f"{block_number}.json"
        if not path.is_file():
            return None
        try:
            with path.open("r", encoding="utf-8") as f:
                return json.load(f)
        except Exception:
            return None

    def read_state_update(self, block_number: int) -> Optional[Dict[str, Any]]:
        path = self.root / f"{block_number}.state_update.json"
        if not path.is_file():
            return None
        try:
            with path.open("r", encoding="utf-8") as f:
                return json.load(f)
        except Exception:
            return None

    def _write_json_atomic(self, target: pathlib.Path, obj: Dict[str, Any]) -> None:
        tmp = target.with_name(f".{target.name}.tmp")
        with tmp.open("w", encoding="utf-8") as f:
            json.dump(obj, f, ensure_ascii=False, indent=2)
            f.write("\n")
        os.replace(str(tmp), str(target))

    def write_block_and_state_update(
        self, block_number: int, block: Dict[str, Any], state_update: Dict[str, Any]
    ) -> Tuple[pathlib.Path, pathlib.Path]:
        block_path = self.root / f"{block_number}.json"
        self._write_json_atomic(block_path, block)
        su_path = self.root / f"{block_number}.state_update.json"
        self._write_json_atomic(su_path, state_update)
        # Update in-memory index
        self._insert_number(block_number)
        return block_path, su_path

    def purge(self) -> int:
        if not self.root.exists():
            return 0
        count = 0
        for p in self.root.glob("*.json"):
            try:
                p.unlink()
                count += 1
            except Exception:
                pass
        for p in self.root.glob("*.state_update.json"):
            try:
                p.unlink()
            except Exception:
                pass
        # Reset in-memory index
        self._numbers_sorted = []
        return count
