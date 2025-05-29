#!/usr/bin/env python3

import argparse
import datetime
import json
import logging
import os
from typing import Optional

import colorlog
from grafana10_objects import (
    alert_expression_model_object,
    alert_query_model_object,
    alert_query_object,
    alert_rule_object,
)
from grafana_client import GrafanaApi
from grafana_client.client import (
    GrafanaBadInputError,
    GrafanaClientError,
    GrafanaException,
    GrafanaServerError,
)
from tenacity import before_sleep_log, retry, stop_after_attempt, wait_fixed


def setup_logger(debug: bool):
    handler = colorlog.StreamHandler()
    handler.setFormatter(
        colorlog.ColoredFormatter(
            "%(asctime)s %(log_color)s%(levelname)s%(reset)s %(message)s",
            log_colors={
                "DEBUG": "blue",
                "INFO": "green",
                "WARNING": "yellow",
                "ERROR": "red",
                "CRITICAL": "bold_red",
            },
        )
    )

    logger = colorlog.getLogger()
    logger.setLevel(logging.DEBUG if debug else logging.INFO)
    logger.handlers = []  # Clear any existing handlers
    logger.addHandler(handler)
    return logger


parser = argparse.ArgumentParser(description="Build And Upload Grafana Alerts")
parser.add_argument(
    "-j",
    "--dev_alerts_file",
    type=str,
    default="./examples/dev_grafana_alerts.json",
    help="Path to the dev alerts file. Default is ./examples/dev_grafana_alerts.json",
)
parser.add_argument(
    "-d",
    "--debug",
    action="store_true",
    help="Enable debug logging. Default is False",
)
parser.add_argument(
    "-u",
    "--grafana_url",
    type=str,
    default="http://localhost:3000",
    help="Grafana URL. Default is http://localhost:3000",
)
parser.add_argument(
    "-n",
    "--dry-run",
    action="store_true",
    help="Dry run, do not upload alerts to Grafana. Default is False",
),
parser.add_argument(
    "-o",
    "--out-dir",
    type=str,
    default="./out",
    help="Output directory. Default is ./out",
)
parser.add_argument(
    "-f",
    "--folder_uid",
    type=str,
    default="",
    help='Provide Grafana folder_uid for the alerts. Default is ""',
)


def create_alert_expression_model(conditions: list[dict[str, any]]):
    logging.debug(f"Creating alert expression model {conditions}")
    model = alert_expression_model_object.copy()
    model["conditions"] = conditions
    logging.debug(f"Alert expression model created: {model}")
    return model


def create_alert_query_model(expr: str):
    logging.debug(f"Creating alert query model {expr}")
    model = alert_query_model_object.copy()
    model["expr"] = expr
    logging.debug(f"Alert query model created: {model}")
    return model


def create_alert_query(
    model: dict[any, any],
    ref_id: str = "A",
    relative_time_range: dict[str, int] = {"from": 600, "to": 0},
    datasource_uid: str = "PBFA97CFB590B2093",
):
    logging.debug(f"Creating alert query {model}")
    alert_query = alert_query_object.copy()
    alert_query["refId"] = ref_id
    alert_query["relativeTimeRange"] = relative_time_range
    alert_query["datasourceUid"] = datasource_uid
    alert_query["model"] = model
    logging.debug(f"Alert query created: {alert_query}")
    return alert_query


def create_alert_rule(
    name: str,
    title: str,
    folder_uid: str,
    rule_group: str,
    interval_sec: int,
    _for: str,
    expr: str,
    conditions: list[dict[str, any]],
):
    logging.debug(f"Creating alert rule {name}")
    alert_rule = alert_rule_object.copy()
    alert_rule["name"] = name
    alert_rule["title"] = title
    alert_rule["folderUID"] = folder_uid
    alert_rule["ruleGroup"] = rule_group
    alert_rule["intervalSec"] = interval_sec
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
    logging.debug(f"Alert rule created: {alert_rule}")
    return alert_rule


def get_all_folders(client: GrafanaApi) -> list[dict[str, any]]:
    logging.debug("Getting all folders")
    return client.folder.get_all_folders()


def get_folder_uid(client: GrafanaApi, title: str) -> Optional[str]:
    """
    Returns the UID of the folder if it exists, otherwise None.
    """
    folders = get_all_folders(client=client)
    for folder in folders:
        if folder["title"] == title:
            return folder["uid"]
    return None


def create_folder_return_uid(client: GrafanaApi, title: str) -> str:
    folder_uid = get_folder_uid(client=client, title=title)
    if folder_uid:
        logging.info(f"Folder '{title}' already exists. Returning existing UID: {folder_uid}")
        return folder_uid
    else:
        logging.info(f"Creating folder '{title}'")
        folder = client.folder.create_folder(title)
        logging.info(f"Folder '{title}' created successfully. {folder}")
        return folder["uid"]


def dump_alert(output_dir: str, alert: dict[str, any]) -> None:
    alert_full_path = f"{output_dir}/{alert['name']}.json".lower().replace(" ", "_")
    os.makedirs(output_dir, exist_ok=True)
    with open(alert_full_path, "w") as f:
        json.dump(alert, f, indent=2)
    logging.info(f'Alert "{alert["name"]}" saved to {alert_full_path}')


def get_alert_rule_group(client: GrafanaApi, folder_uid: str, group_uid: str) -> str:
    logging.debug(f'Getting alert rule group "{group_uid}"')
    rule_group = client.alertingprovisioning.get_rule_group(
        folder_uid=folder_uid, group_uid=group_uid
    )
    logging.debug(f"Got alert group: {rule_group}")
    return rule_group


@retry(
    stop=stop_after_attempt(10),
    wait=wait_fixed(2),
    before_sleep=before_sleep_log(logging.getLogger(), logging.DEBUG),
)
def update_alert_rule_group(
    client: GrafanaApi,
    folder_uid: str,
    group_uid: str,
    alertrule_group: dict[any, any],
    disable_provenance=True,
) -> None:
    logging.debug(f'Updating alert rule group "{group_uid}"')

    try:
        client.alertingprovisioning.update_rule_group(
            folder_uid=folder_uid,
            group_uid=group_uid,
            alertrule_group=alertrule_group,
            disable_provenance=disable_provenance,
        )
        logging.info(f"Successfully updated alert rule group {group_uid}")
    except Exception as e:
        logging.error(f"Failed to update alert rule group {group_uid}: {e}")
        raise


def main():
    args = parser.parse_args()
    logger = setup_logger(debug=args.debug)
    start_time = datetime.datetime.now()
    logger.info(
        f'Starting to build grafana dashboard, time is {start_time.strftime("%Y-%m-%d %H:%M:%S")}'
    )

    with open(args.dev_alerts_file, "r") as f:
        dev_alerts = json.load(f)

    if not args.dry_run:
        client = GrafanaApi.from_url(args.grafana_url)
        folder_uid = create_folder_return_uid(client=client, title="Sequencer")
    else:
        folder_uid = args.folder_uid

    alerts = []

    for dev_alert in dev_alerts["alerts"]:
        alerts.append(
            create_alert_rule(
                name=dev_alert["name"],
                title=dev_alert["title"],
                folder_uid=folder_uid,
                interval_sec=dev_alert["intervalSec"],
                rule_group=dev_alert["ruleGroup"],
                _for=dev_alert["for"],
                expr=dev_alert["expr"],
                conditions=dev_alert["conditions"],
            )
        )

    for alert in alerts:
        if args.debug:
            logging.debug(json.dumps(alert))
        if not args.dry_run:
            try:
                client.alertingprovisioning.create_alertrule(
                    alertrule=alert, disable_provenance=True
                )
                logging.info(f'Alert "{alert["name"]}" uploaded to Grafana successfully')

            except GrafanaBadInputError as e:
                if "alerting.alert-rule.conflict" in e.message:
                    logging.warning(
                        f'Alert "{alert["name"]}" already exists. Skipping creation. Conflict message: {e.message}'
                    )
                else:
                    # Handle other bad input errors
                    logging.error(
                        f'Failed to create alert "{alert["name"]}". Bad input: {e.message}'
                    )
            except GrafanaClientError as e:
                # Handle other client-side errors (e.g., invalid request)
                logging.error(
                    f'Failed to create alert "{alert["name"]}". Client error: {e.message}'
                )
            except GrafanaServerError as e:
                # Handle server-side errors (5xx errors)
                logging.error(
                    f'Failed to create alert "{alert["name"]}". Server error: {e.message}'
                )
            except GrafanaException as e:
                # Catch any other Grafana-related exceptions
                logging.error(
                    f'Failed to create alert "{alert["name"]}". Grafana error: {e.message}'
                )
            except Exception as e:
                # Catch any other exceptions (non-Grafana-related)
                logging.error(f'Failed to create alert "{alert["name"]}". Unexpected error: {e}')

            try:
                group_uid = alert["ruleGroup"]
                rule_group = get_alert_rule_group(
                    client=client, folder_uid=folder_uid, group_uid=group_uid
                )
                if rule_group["interval"] != alert["intervalSec"]:
                    rule_group["interval"] = alert["intervalSec"]
                    update_alert_rule_group(
                        client=client,
                        folder_uid=folder_uid,
                        group_uid=group_uid,
                        alertrule_group=rule_group,
                    )
                    logging.info(f'Alert rule group "{group_uid}" updated successfully')
            except Exception as e:
                logging.error(f'Failed to update alert rule group "{alert["ruleGroup"]}". {e}')

        dump_alert(output_dir=args.out_dir, alert=alert)

    logging.info(
        f'Done building grafana alerts, time is {datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S")}'
    )


if __name__ == "__main__":
    main()
