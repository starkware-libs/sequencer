import json
import os
from typing import Any, Dict


class GrafanaDashboard:
    def __init__(self, dashboard_file_path: str):
        self.dashboard_path = os.path.abspath(dashboard_file_path)

    def get_dashboard(self) -> Dict[str, Any]:
        with open(self.dashboard_path, "r") as f:
            value: Dict[str, Any] = json.load(f)
            return value
