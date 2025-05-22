import datetime
import json
import logging
import os
import time

import click
import requests
from grafana10_objects import empty_dashboard, row_object, templating_object


def create_grafana_panel(panel: dict, panel_id: int, y_position: int, x_position: int) -> dict:
    grafana_panel = {
        "id": panel_id,
        "type": panel["type"],
        "title": panel["title"],
        "description": panel.get("description", ""),
        "gridPos": {"h": 6, "w": 12, "x": x_position, "y": y_position},
        "targets": [
            {
                "expr": panel["expr"] if isinstance(panel["expr"], str) else None,
                "refId": chr(65 + panel_id % 26),
            }
        ],
        "fieldConfig": {
            "defaults": {
                "unit": "none",
                "thresholds": {
                    "mode": "absolute",
                    "steps": [
                        {"color": "green", "value": None},
                        {"color": "orange", "value": 70},
                        {"color": "red", "value": 90},
                    ],
                },
            }
        },
    }
    return grafana_panel


def get_next_position(x_position, y_position):
    """Helper function to calculate next position for the panel."""
    panel_grid_pos_width = 12

    if x_position == panel_grid_pos_width:
        x_position = 0
        y_position += 6
    else:
        x_position += panel_grid_pos_width

    return x_position, y_position


def dashboard_file_name(out_dir: str, dashboard_name: str) -> str:
    file_name = dashboard_name.replace(" ", "_").lower()
    return f"{out_dir}/{file_name}.json"


def create_dashboard(dashboard_name: str, dev_dashboard: json) -> dict:
    dashboard = empty_dashboard.copy()
    templating = templating_object.copy()
    panel_id = 1
    x_position = 0
    y_position = 0
    dashboard["title"] = dashboard_name
    dashboard["templating"] = templating

    for row_title, panels in dev_dashboard.items():
        row_panel = row_object.copy()
        row_panel["title"] = row_title
        row_panel["id"] = panel_id
        row_panel["gridPos"]["y"] = y_position
        row_panel["panels"] = []
        panel_id += 1
        x_position = 0
        y_position += 1
        dashboard["panels"].append(row_panel)

        for panel in panels:
            grafana_panel = create_grafana_panel(panel, panel_id, y_position, x_position)
            row_panel["panels"].append(grafana_panel)
            panel_id += 1
            x_position, y_position = get_next_position(x_position, y_position)

    return {"dashboard": dashboard}


def upload_dashboards_local(dashboard: dict) -> None:
    retry = 0

    while retry <= 10:
        try:
            res = requests.post(
                "http://localhost:3000/api/dashboards/db",
                json={**dashboard, **{"overwrite": True}},
            )
            if res.status_code != 200:
                print(f"Failed to upload dashboard. {res.json()}")
                break
            print("Dashboard uploaded successfully.")
            print(f"you can view the dashboard at: http://localhost:3000{res.json()['url']}")
            break
        except requests.exceptions.ConnectionError:
            print("Grafana is not ready yet. Retrying...")
            retry += 1
            time.sleep(5)
            continue


@click.group()
def cli():
    pass


@cli.command()
@click.option("-j", "--dev_json_file", default="./dev_dashboard.json")
@click.option("-d", "--debug", is_flag=True, default=False)
@click.option("-u", "--upload", is_flag=True, default=False)
@click.option("-o", "--out_dir", default="./out")
def builder(dev_json_file, out_dir, upload, debug) -> None:
    dashboards = []

    # Logging
    if debug:
        logging.basicConfig(level=logging.DEBUG, format="%(asctime)s - %(levelname)s - %(message)s")
    else:
        logging.basicConfig(level=logging.INFO, format="%(asctime)s - %(levelname)s - %(message)s")
    start_time = datetime.datetime.now()
    logging.info(
        f'Starting to build grafana dashboard, time is {start_time.strftime("%Y-%m-%d %H:%M:%S")}'
    )

    # Load json file
    with open(dev_json_file, "r") as f:
        dev_json = json.load(f)

    for dashboard_name in dev_json.keys():
        dashboards.append(
            [
                dashboard_name,
                create_dashboard(
                    dashboard_name=dashboard_name,
                    dev_dashboard=dev_json[dashboard_name],
                ),
            ]
        )
    print(dashboards)

    # Write the grafana dashboard
    os.makedirs(out_dir, exist_ok=True)
    for dashboard_name, dashboard in dashboards:
        with open(dashboard_file_name(out_dir, dashboard_name), "w") as f:
            json.dump(dashboard, f, indent=4)
        if upload:
            upload_dashboards_local(dashboard=dashboard)
    logging.info(
        f'Done building grafana dashboard, time is {datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S")}'
    )


if __name__ == "__main__":
    cli()
