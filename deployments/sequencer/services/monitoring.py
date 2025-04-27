import json
import os


class GrafanaDashboard:
    def __init__(self, dashboard_file_path: str):
        self.dashboard_path = os.path.abspath(dashboard_file_path)

    def get_dashboard(self):
        with open(self.dashboard_path, "r") as f:
            return json.load(f)
