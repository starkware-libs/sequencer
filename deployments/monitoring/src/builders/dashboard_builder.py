import argparse
import json
import os
import time
from typing import Any, Dict, List, Tuple

import requests
from src.common.grafana10_objects import empty_dashboard, row_object, templating_object
from src.common.helpers import get_logger


def create_grafana_panel(
    panel: Dict[str, Any], panel_id: int, y_position: int, x_position: int
) -> Dict[str, Any]:
    exprs = panel["exprs"]

    # Validate expressions input
    ASCII_A = ord("A")
    MAX_REFIDS = ord("Z") - ASCII_A + 1
    assert len(exprs) <= MAX_REFIDS, (
        f"Too many expressions in panel '{panel.get('title', '')}': "
        f"{len(exprs)} expressions provided, max is {MAX_REFIDS}.\nExpressions:\n"
        + "\n".join(f"{i + 1}. {expr}" for i, expr in enumerate(exprs))
    )

    # Generate targets with unique refIds A–Z
    targets = [
        {
            "expr": expr,
            "refId": chr(ASCII_A + i),  # 'A' to 'Z'
        }
        for i, expr in enumerate(exprs)
    ]

    grafana_panel = {
        "id": panel_id,
        "type": panel["type"],
        "title": panel["title"],
        "description": panel.get("description", ""),
        "gridPos": {"h": 6, "w": 12, "x": x_position, "y": y_position},
        "targets": targets,
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


def get_next_position(x_position: int, y_position: int) -> Tuple[int, int]:
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


def create_dashboard(
    dashboard_name: str, dev_dashboard: Dict[str, List[Dict[str, Any]]]
) -> Dict[str, Dict[str, Any]]:
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
        assert isinstance(row_panel["gridPos"], dict)
        row_panel["gridPos"]["y"] = y_position
        row_panel["panels"] = []
        panel_id += 1
        x_position = 0
        y_position += 1
        assert isinstance(dashboard["panels"], list)
        dashboard["panels"].append(row_panel)

        assert isinstance(row_panel["panels"], list)
        for panel in panels:
            grafana_panel = create_grafana_panel(panel, panel_id, y_position, x_position)
            row_panel["panels"].append(grafana_panel)
            panel_id += 1
            x_position, y_position = get_next_position(x_position, y_position)

    return {"dashboard": dashboard}


# TODO(Idan Shamam): Use Grafana Client to upload the dashboards
def upload_dashboards_local(dashboard: Dict[str, Any]) -> None:
    retry = 0

    while retry <= 10:
        try:
            res = requests.post(
                "http://localhost:3000/api/dashboards/db",
                json={**dashboard, **{"overwrite": True}},
            )
            if res.status_code != 200:
                logger.error(f"Failed to upload dashboard. {res.json()}")
                break
            logger.info("Dashboard uploaded successfully.")
            logger.info(f"you can view the dashboard at: http://localhost:3000{res.json()['url']}")
            break
        except requests.exceptions.ConnectionError:
            logger.info("Grafana is not ready yet. Retrying...")
            retry += 1
            time.sleep(5)
            continue


def dashboard_builder(args: argparse.Namespace) -> None:
    global logger
    logger = get_logger(name="dashboard_builder", debug=args.debug)

    logger.info(f"Starting to build grafana dashboard")

    # Load json file
    with open(args.dev_dashboards_file, "r") as f:
        dev_json: Dict[str, Dict[str, Any]] = json.load(f)

    dashboards: List[Tuple[str, Dict[str, Dict[str, Any]]]] = []
    for dashboard_name in dev_json.keys():
        dashboards.append(
            (
                dashboard_name,
                create_dashboard(
                    dashboard_name=dashboard_name,
                    dev_dashboard=dev_json[dashboard_name],
                ),
            )
        )
    logger.debug(json.dumps(dashboards, indent=4))
    # Write the grafana dashboard
    for dashboard_name, dashboard in dashboards:
        if args.out_dir:
            output_dir = f"{args.out_dir}/dashboards"
            os.makedirs(output_dir, exist_ok=True)
            with open(dashboard_file_name(output_dir, dashboard_name), "w") as f:
                json.dump(dashboard, f, indent=4)
        if not args.dry_run:
            upload_dashboards_local(dashboard=dashboard)

    logger.info("Done building grafana dashboards")
