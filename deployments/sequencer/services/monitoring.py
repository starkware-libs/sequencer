import json
import os
from pathlib import Path


class GrafanaDashboard:
    def __init__(self, dashboard_file_path: str):
        self.dashboard_path = os.path.abspath(dashboard_file_path)

    def load_dashboard(self):
        with open(self.dashboard_path, "r") as f:
            return json.load(f)


class GrafanaAlertGroup:
    def __init__(self, alerts_folder_path: str):
        self.alerts_folder_path = Path(alerts_folder_path)

    def get_alert_files(self):
        return [file for file in self.alerts_folder_path.glob("*.json")]

    def load_alert(self, alert_file_path: str):
        with open(alert_file_path, "r") as f:
            return json.load(f)
