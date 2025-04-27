import json
import os
import subprocess


class GrafanaDashboard:
    ROOT_DIR = os.path.join(
        os.path.dirname(os.path.abspath(__file__)), "../../../Monitoring/sequencer/"
    )

    def __init__(self, dashboard_file: str):
        self.dashboard_path = os.path.join(self.ROOT_DIR, dashboard_file)

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
