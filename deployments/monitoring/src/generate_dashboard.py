#!/bin/env python3
"""Generate Grafana dashboards from a json model."""

import argparse
import json
import os
import time
import requests

from typing import Dict, List
from grafanalib.core import (
    Dashboard,
    TimeSeries,
    GaugePanel,
    Stat,
    Target,
    GridPos,
    Row,
    OPS_FORMAT,
)


class DashboardEncoder(json.JSONEncoder):
    """Encode dashboard objects."""

    def default(self, obj):
        to_json_data = getattr(obj, "to_json_data", None)
        if to_json_data is not None:
            return to_json_data()
        return json.JSONEncoder.default(self, obj)


def create_timeseries_panel(panel):
    return TimeSeries(
        title=panel["name"],
        dataSource="Prometheus",
        targets=[
            Target(
                expr=panel["expr"],
                legendFormat="{{ handler }}",
                refId="A",
            ),
        ],
        unit=OPS_FORMAT,
        gridPos=GridPos(h=8, w=16, x=0, y=10),
    )


def create_stat_panel(panel):
    return Stat(
        title=panel["name"],
        dataSource="Prometheus",
        targets=[
            Target(
                expr=panel["expr"],
                legendFormat="{{ handler }}",
                refId="A",
            ),
        ],
        gridPos=GridPos(h=8, w=16, x=0, y=10),
    )


def create_panels(dashboard: Dashboard, model: Dict):
    row = Row(title=model["name"])
    for panel in model["panels"]:
        if panel["panel_type"] == "TimeSeries":
            p = create_timeseries_panel(panel=panel)
        elif panel["panel_type"] == "Stat":
            p = create_stat_panel(panel=panel)
        row.panels.append(p)

    dashboard.rows.append(row)


def generate_dashboard(model: Dict):
    dashboard = Dashboard(
        title=model["name"],
        description=model["description"],
        timezone="browser",
        rows=[],
    )

    for row in model["rows"]:
        create_panels(dashboard=dashboard, model=row)

    return dashboard.auto_panel_ids()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--input-file",
        required=True,
        type=str,
        help="Required: Specify the json modeling path.",
    )
    parser.add_argument(
        "--output-dir", type=str, help="Optional: Path to generated dashboard files."
    )
    parser.add_argument(
        "--upload",
        action="store_true",
        help="Optional: Specify the enironment (e.g., dev, prod)",
    )

    args = parser.parse_args()

    model = json.load(open(args.input_file))
    for dashboard in model["dashboards"]:
        generated_dashboard = json.dumps(
            generate_dashboard(model=dashboard).to_json_data(),
            sort_keys=True,
            indent=2,
            cls=DashboardEncoder,
        )
        output_file = os.path.join(args.output_dir, f"{dashboard['name']}.json")
        print(f"Dashboard generated successfully. output file: {output_file}")
        with open(output_file, "w") as f:
            f.write(generated_dashboard)

        if args.upload:
            # upload the dashboard to grafana
            retry = 0
            while retry <= 10:
                try:
                    res = requests.post(
                        "http://localhost:3000/api/dashboards/db",
                        json=dict(dashboard=json.loads(generated_dashboard)),
                    )
                    if res.status_code != 200:
                        print(f"Failed to upload dashboard. {res.json()}")
                        break
                    print("Dashboard uploaded successfully.")
                    print(
                        f"you can view the dashboard at: http://localhost:3000{res.json()['url']}"
                    )
                    break
                except requests.exceptions.ConnectionError:
                    print("Grafana is not ready yet. Retrying...")
                    retry += 1
                    time.sleep(5)
                    continue


if __name__ == "__main__":
    main()
