import json
import os
import subprocess


class GrafanaDashboard:
    def __init__(self, dashboard_file_path: str):
        self.dashboard_path = os.path.abspath(dashboard_file_path)

    def get_dashboard(self):
        print("Generating dashboard file:")
        generate_dashboard_json_cmd = [
            "python",
            "./deployments/monitoring/src/dashboard_builder.py",
            "builder",
            "-j",
            "./Monitoring/sequencer/dev_grafana.json",
            "-o",
            self.ROOT_DIR,
        ]
        print(generate_dashboard_json_cmd, flush=True)
        subprocess.run(generate_dashboard_json_cmd, check=True)

        with open(self.dashboard_path, "r") as f:
            return json.load(f)
