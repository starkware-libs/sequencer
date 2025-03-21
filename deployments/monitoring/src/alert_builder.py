#!/usr/bin/env python3

import argparse
import logging
import datetime
import json
import os
import requests
import time
from grafana_client import GrafanaApi

from grafana10_objects import (
    alert_rule_object,
    alert_query_object,
    alert_query_model_object,
    alert_expression_model_object,
)


def create_alert_expression_model(conditions: list[dict[str, any]]):
    model = alert_expression_model_object.copy()
    model["conditions"] = conditions
    return model


def create_alert_query_model(expr: str):
    model = alert_query_model_object.copy()
    model["expr"] = expr
    return model


def create_alert_query(
    model: dict[any, any],
    ref_id: str = "A",
    relative_time_range: dict[str, int] = {"from": 600, "to": 0},
    datasource_uid: str = "PBFA97CFB590B2093",
):
    alert_query = alert_query_object.copy()
    alert_query["refId"] = ref_id
    alert_query["relativeTimeRange"] = relative_time_range
    alert_query["datasourceUid"] = datasource_uid
    alert_query["model"] = model
    return alert_query


def create_alert_rule(
    name: str,
    title: str,
    folder_uid: str,
    rule_group: str,
    _for: str,
    expr: str,
    conditions: list[dict[str, any]],
):
    alert_rule = alert_rule_object.copy()
    alert_rule["name"] = name
    alert_rule["title"] = title
    alert_rule["folderUID"] = folder_uid
    alert_rule["ruleGroup"] = rule_group
    alert_rule["for"] = _for
    alert_rule["data"] = [
        create_alert_query(model=create_alert_query_model(expr=expr)),
        create_alert_query(
            ref_id="B",
            relative_time_range={"from": 1, "to": 0},
            datasource_uid="__expr__",
            model=create_alert_expression_model(conditions=conditions),
        ),
    ]

    return alert_rule


def get_all_folders(client: GrafanaApi) -> list[dict[str, any]]:
    return client.folder.get_all_folders()


def is_folder_exists(client: GrafanaApi, title: str) -> bool:
    folders = get_all_folders(client=client)
    return True if any(folder["title"] == title for folder in folders) else False


def create_folder_return_uid(client: GrafanaApi, title: str) -> str:
    if is_folder_exists(client=client, title=title):
        return
    else:
        print(f"Creating folder {title}")
        folder = client.folder.create_folder(title)
        print(f"Folder {title} created successfully. {folder}")
    return folder["uid"]


def main():
    client = GrafanaApi.from_url("http://localhost:3000")
    alerts = []
    folder_uid = create_folder_return_uid(client=client, title="Sequencer")

    dev_alert_path = "/home/idan/workspace/repos/starkware-libs/sequencer/deployments/monitoring/testing/dev_grafana_alerts.json"
    with open(dev_alert_path, "r") as f:
        dev_alerts = json.load(f)

    for dev_alert in dev_alerts["alerts"]:
        alerts.append(
            create_alert_rule(
                name=dev_alert["name"],
                title=dev_alert["title"],
                folder_uid=folder_uid,
                rule_group=dev_alert["ruleGroup"],
                _for=dev_alert["for"],
                expr=dev_alert["expr"],
                conditions=dev_alert["conditions"],
            )
        )

    for alert in alerts:
        print(json.dumps(alert, indent=2))
        client.alertingprovisioning.create_alertrule(
            alertrule=alert, disable_provenance=True
        )


if __name__ == "__main__":
    main()
