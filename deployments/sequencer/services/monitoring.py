import json
import os


class GrafanaDashboard:
    ROOT_DIR = os.path.join(
        os.path.dirname(os.path.abspath(__file__)), "../../../Monitoring/sequencer/"
    )

    def __init__(self, dashboard_file: str):
        self.dashboard_path = os.path.join(self.ROOT_DIR, dashboard_file)

    def get(self):
        with open(self.dashboard_path, "r") as f:
            return json.load(f)
