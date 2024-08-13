#!/bin/env python3
import os

from generate_changelog import CRATES


if __name__ == "__main__":
    dir = os.path.dirname(os.path.abspath(__file__))
    crates_path = os.path.join(dir, "../crates")
    actual_crates = {f.name for f in os.scandir(crates_path) if f.is_dir()}
    assert set(actual_crates).issubset(CRATES)
