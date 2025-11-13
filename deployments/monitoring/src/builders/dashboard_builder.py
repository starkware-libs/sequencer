import argparse
import json
import os
import time

import copy
import requests
from common.grafana10_objects import empty_dashboard, row_object, templating_object
from common.helpers import EnvironmentName, env_to_gcp_project_name, get_logger
from urllib.parse import quote

MAX_ALLOWED_JSON_SIZE = 1024 * 1024  # 1MB


def create_grafana_panel(panel: dict, panel_id: int, y_position: int, x_position: int) -> dict:
    exprs = panel["exprs"]

    # Validate expressions input
    ASCII_A = ord("A")
    MAX_REFIDS = ord("Z") - ASCII_A + 1
    assert len(exprs) <= MAX_REFIDS, (
        f"Too many expressions in panel '{panel.get('title', '')}': "
        f"{len(exprs)} expressions provided, max is {MAX_REFIDS}.\nExpressions:\n"
        + "\n".join(f"{i + 1}. {expr}" for i, expr in enumerate(exprs))
    )

    extra = panel.get("extra_params", {})
    unit = extra.get("unit", "none")
    show_percent_change = extra.get("show_percent_change", False)
    log_query = extra.get("log_query", "")
    log_comment = extra.get("log_comment", "")
    thresholds = extra.get("thresholds", {})
    query_parts = [
        f"resource.labels.namespace_name=~%22^%28${{namespace:pipe}}%29$%22",
        quote(log_query),
    ]
    if log_comment:
        query_parts.append(quote(log_comment))
    query_value = "%0A".join(query_parts)
    # TODO(Ron): Turn link into variable to save space in the json file
    link = "\n".join(
        [
            "https://console.cloud.google.com/logs/query;",
            f"query={query_value};",
            "summaryFields=resource%252Flabels%252Fnamespace_name,resource%252Flabels%252Fcontainer_name;",
            "timeRange=${__from:date:iso}%2F${__to:date:iso}",
            "?project=${gcp_project}",
        ]
    )
    legends = extra.get("legends", [])
    display_name_override_value = (
        "${__field.labels.namespace}"
        if panel["type"] == "stat"
        else "${__field.labels.namespace} | ${__field.labels.location}"
    )

    # Generate targets with unique refIds A–Z
    targets = [
        {
            "expr": expr,
            "refId": chr(ASCII_A + i),  # 'A' to 'Z'
            **({"legendFormat": f"{legends[i]} " + "{{namespace}}"} if legends else {}),
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
                "color": {"mode": "palette-classic"},
                "unit": unit,
                "thresholds": thresholds,
            },
            "overrides": [
                # Override the pod display name to show only namespace (and sometimes location) labels
                {
                    "matcher": {"id": "byRegexp", "options": ".*location.*"},
                    "properties": [{"id": "displayName", "value": display_name_override_value}],
                }
            ],
        },
        "links": ([{"url": link, "title": "GCP Logs", "targetBlank": True}]),
        "transformations": [
            # Renames labels of the form {label="value"} to just "value"
            {
                "id": "renameByRegex",
                "options": {"regex": '^\\{[^=]+=\\"([^\\"]+)\\"\\}$', "renamePattern": "$1"},
            },
            # Used twice to remove up to 2 instances of cluster and namespace labels, since it is
            # not possible to remove all in one transformation
            remove_cluster_and_namespace_from_display_name(),
            remove_cluster_and_namespace_from_display_name(),
        ],
    }

    if thresholds:
        grafana_panel["fieldConfig"]["defaults"]["color"] = {"mode": "thresholds"}

    if panel["type"] == "stat":
        grafana_panel["options"] = {
            "textMode": "value_and_name",
            "showPercentChange": show_percent_change,
        }

    return grafana_panel


def remove_cluster_and_namespace_from_display_name():
    return {
        "id": "renameByRegex",
        "options": {
            # Remove 'cluster' and 'namespace' label from display name if it is a panel with
            # combined namespaces (meaning it contains 'cluster' but not 'location').
            "regex": "^(.*)\{(?=[^}]*cluster)(?![^}]*location)[^}]*\}(.*)$",
            "renamePattern": "$1$2",
        },
    }


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


def make_gcp_project_var(gcp_project_value: str) -> dict:
    return {
        "type": "constant",
        "name": "gcp_project",
        "query": gcp_project_value,
    }


def create_dashboard(dashboard_name: str, dev_dashboard: json, env: EnvironmentName) -> dict:
    dashboard = empty_dashboard.copy()
    templating = templating_object.copy()
    templating["list"].append(make_gcp_project_var(env_to_gcp_project_name(env)))
    panel_id = 1
    x_position = 0
    y_position = 0
    dashboard["title"] = dashboard_name
    dashboard["templating"] = templating

    for row_title, value in dev_dashboard.items():
        panels = value.get("panels")
        collapsed = bool(value.get("collapsed"))
        row_panel = copy.deepcopy(row_object)
        row_panel["title"] = row_title
        row_panel["id"] = panel_id
        row_panel["gridPos"]["y"] = y_position
        row_panel["panels"] = []
        row_panel["collapsed"] = collapsed
        panel_id += 1
        x_position = 0
        y_position += 1
        dashboard["panels"].append(row_panel)
        # Rows that aren't collapsed require its panels to be added directly to the dashboard
        target = row_panel["panels"] if collapsed else dashboard["panels"]

        for panel in panels:
            grafana_panel = create_grafana_panel(panel, panel_id, y_position, x_position)
            target.append(grafana_panel)
            panel_id += 1
            x_position, y_position = get_next_position(x_position, y_position)

    return {"dashboard": dashboard}


# TODO(Idan Shamam): Use Grafana Client to upload the dashboards
def upload_dashboards_local(dashboard: dict) -> None:
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
        dev_json = json.load(f)

    dashboards = []
    for dashboard_name in dev_json.keys():
        dashboards.append(
            [
                dashboard_name,
                create_dashboard(
                    dashboard_name=dashboard_name,
                    dev_dashboard=dev_json[dashboard_name],
                    env=EnvironmentName(args.env),
                ),
            ]
        )
    logger.debug(json.dumps(dashboards, indent=4))
    # Write the grafana dashboard
    for dashboard_name, dashboard in dashboards:
        if args.out_dir:
            output_dir = f"{args.out_dir}/dashboards"
            os.makedirs(output_dir, exist_ok=True)
            json_data = json.dumps(dashboard, indent=1, ensure_ascii=False)
            assert len(json_data) < MAX_ALLOWED_JSON_SIZE, "Grafana dashboard JSON is too large"
            with open(dashboard_file_name(output_dir, dashboard_name), "w", encoding="utf-8") as f:
                f.write(json_data)
        if not args.dry_run:
            upload_dashboards_local(dashboard=dashboard)

    logger.info("Done building grafana dashboards")
