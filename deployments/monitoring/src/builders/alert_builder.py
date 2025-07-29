#!/usr/bin/env python3

import argparse
import json
import os
from typing import Optional

<<<<<<< HEAD
from common import const
from common.grafana10_objects import (
    alert_expression_model_object,
    alert_query_model_object,
    alert_query_object,
    alert_rule_object,
)
from common.helpers import get_logger
||||||| 937a3d39a
from common.grafana10_objects import (
    alert_expression_model_object,
    alert_query_model_object,
    alert_query_object,
    alert_rule_object,
)
from common.helpers import get_logger
=======
>>>>>>> origin/main-v0.14.0
from grafana_client import GrafanaApi
<<<<<<< HEAD
from grafana_client.client import (
    GrafanaBadInputError,
    GrafanaClientError,
    GrafanaException,
    GrafanaServerError,
)
||||||| 937a3d39a
from grafana_client.client import (
    GrafanaBadInputError,
    GrafanaClientError,
    GrafanaException,
    GrafanaServerError,
)
from common import const
=======
from grafana_client.client import (GrafanaBadInputError, GrafanaClientError,
                                   GrafanaException, GrafanaServerError)
>>>>>>> origin/main-v0.14.0
from tenacity import before_sleep_log, retry, stop_after_attempt, wait_fixed

from common import const
from common.grafana10_objects import (alert_expression_model_object,
                                      alert_query_model_object,
                                      alert_query_object, alert_rule_object)
from common.helpers import (EnvironmentName, alert_env_filename_suffix,
                            get_logger)


def create_alert_expression_model(conditions: list[dict[str, any]]):
    logger.debug(f"Creating alert expression model {conditions}")
    model = alert_expression_model_object.copy()
    model["conditions"] = conditions
    logger.debug(f"Alert expression model created: {model}")
    return model


def create_alert_query_model(expr: str):
    logger.debug(f"Creating alert query model {expr}")
    model = alert_query_model_object.copy()
    model["expr"] = expr
    logger.debug(f"Alert query model created: {model}")
    return model


def create_alert_query(
    model: dict[any, any],
    datasource_uid: str,
    ref_id: str = "A",
    relative_time_range: dict[str, int] = {"from": 600, "to": 0},
):
    logger.debug(f"Creating alert query {model}")
    alert_query = alert_query_object.copy()
    alert_query["refId"] = ref_id
    alert_query["relativeTimeRange"] = relative_time_range
    alert_query["datasourceUid"] = datasource_uid
    alert_query["model"] = model
    logger.debug(f"Alert query created: {alert_query}")
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
    datasource_uid: str,
    labels: dict[str, str] = {},
):
    logger.debug(f"Creating alert rule {name}")
    alert_rule = alert_rule_object.copy()
    alert_rule["name"] = name
    alert_rule["title"] = title
    alert_rule["folderUID"] = folder_uid
    alert_rule["ruleGroup"] = rule_group
    alert_rule["intervalSec"] = interval_sec
    alert_rule["for"] = _for
    alert_rule["labels"] = labels
    alert_rule["data"] = [
        create_alert_query(
            datasource_uid=datasource_uid, model=create_alert_query_model(expr=expr)
        ),
        create_alert_query(
            ref_id="B",
            relative_time_range={"from": 1, "to": 0},
            datasource_uid="__expr__",
            model=create_alert_expression_model(conditions=conditions),
        ),
    ]
    logger.debug(f"Alert rule created: {alert_rule}")
    return alert_rule


def get_all_folders(client: GrafanaApi) -> list[dict[str, any]]:
    logger.debug("Getting all folders")
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
        logger.info(f"Folder '{title}' already exists. Returning existing UID: {folder_uid}")
        return folder_uid
    else:
        logger.info(f"Creating folder '{title}'")
        folder = client.folder.create_folder(title)
        logger.info(f"Folder '{title}' created successfully. {folder}")
        return folder["uid"]


def dump_alert(output_dir: str, alert: dict[str, any]) -> None:
    alert_full_path = f"{output_dir}/{alert['name']}.json".lower().replace(" ", "_")
    os.makedirs(output_dir, exist_ok=True)
    with open(alert_full_path, "w") as f:
        json.dump(alert, f, indent=2)
    logger.info(f'Alert "{alert["name"]}" saved to {alert_full_path}')


def get_alert_rule_group(client: GrafanaApi, folder_uid: str, group_uid: str) -> str:
    logger.debug(f'Getting alert rule group "{group_uid}"')
    rule_group = client.alertingprovisioning.get_rule_group(
        folder_uid=folder_uid, group_uid=group_uid
    )
    logger.debug(f"Got alert group: {rule_group}")
    return rule_group


@retry(
    stop=stop_after_attempt(10),
    wait=wait_fixed(2),
    before_sleep=before_sleep_log(logger=get_logger(name="tenacity_retry"), log_level="DEBUG"),
)
def update_alert_rule_group(
    client: GrafanaApi,
    folder_uid: str,
    group_uid: str,
    alertrule_group: dict[any, any],
    disable_provenance=True,
) -> None:
    logger.debug(f'Updating alert rule group "{group_uid}"')

    try:
        client.alertingprovisioning.update_rule_group(
            folder_uid=folder_uid,
            group_uid=group_uid,
            alertrule_group=alertrule_group,
            disable_provenance=disable_provenance,
        )
        logger.info(f"Successfully updated alert rule group {group_uid}")
    except Exception as e:
        logger.error(f"Failed to update alert rule group {group_uid}: {e}")
        raise


def inject_expr_placeholders(expr: str, cluster: str, namespace: str) -> str:
    return expr.replace(
        const.ALERT_RULE_EXPRESSION_PLACEHOLDER,
        '{{namespace="{0}", cluster="{1}"}}'.format(namespace, cluster),
    )


def remove_expr_placeholder(expr: str) -> str:
    return expr.replace(const.ALERT_RULE_EXPRESSION_PLACEHOLDER, "")


# TODO(Tsabary): remove the vanilla path option once we transition to per-env file.
def resolve_dev_alerts_file_path(path: str, suffix: str) -> str:
    """
    Resolve a JSON path:
    - If the original file exists, return it.
    - Otherwise, check for `<name>_<suffix>.json`.
    - Raise an error if neither exists.
    """
    if os.path.isfile(path):
        return path

    # Insert suffix before `.json`
    base, ext = os.path.splitext(path)
    if ext.lower() != ".json":
        raise ValueError(f"Expected a .json file, got: {path}")

    alternative_path = f"{base}_{suffix}{ext}"
    if os.path.isfile(alternative_path):
        return alternative_path

    raise FileNotFoundError(f"Neither '{path}' nor '{alternative_path}' exists.")


def alert_builder(args: argparse.Namespace):
    global logger
    logger = get_logger(name="alert_builder", debug=args.debug)

    suffix = alert_env_filename_suffix(env=EnvironmentName(args.env))
    alert_file_path = resolve_dev_alerts_file_path(path=args.dev_alerts_file, suffix=suffix)

    with open(alert_file_path, "r") as f:
        dev_alerts = json.load(f)

    if not args.dry_run:
        client = GrafanaApi.from_url(args.grafana_url)
        folder_uid = create_folder_return_uid(client=client, title="Sequencer")
    else:
        folder_uid = args.folder_uid

    alerts = []

    for dev_alert in dev_alerts["alerts"]:
        if args.namespace and args.cluster:
            expr = inject_expr_placeholders(
                expr=dev_alert["expr"], namespace=args.namespace, cluster=args.cluster
            )
        else:
            expr = remove_expr_placeholder(expr=dev_alert["expr"])
        alerts.append(
            create_alert_rule(
                name=dev_alert["name"],
                title=dev_alert["title"],
                folder_uid=folder_uid,
                interval_sec=dev_alert["intervalSec"],
                rule_group=dev_alert["ruleGroup"],
                _for=dev_alert["for"],
                expr=expr,
                conditions=dev_alert["conditions"],
                datasource_uid=args.datasource_uid,
                labels={"og_priority": dev_alert["severity"], "environment": args.env},
            )
        )

    for alert in alerts:
        if args.debug:
            logger.debug(json.dumps(alert))
        if not args.dry_run:
            try:
                client.alertingprovisioning.create_alertrule(
                    alertrule=alert,
                    disable_provenance=True,
                )
                logger.info(f'Alert "{alert["name"]}" uploaded to Grafana successfully')

            except GrafanaBadInputError as e:
                if "alerting.alert-rule.conflict" in e.message:
                    logger.info(f'Alert "{alert["name"]}" already exists. Skipping creation.')
                else:
                    # Handle other bad input errors
                    logger.error(
                        f'Failed to create alert "{alert["name"]}". Bad input: {e.message}'
                    )
            except GrafanaClientError as e:
                # Handle other client-side errors (e.g., invalid request)
                logger.error(f'Failed to create alert "{alert["name"]}". Client error: {e.message}')
            except GrafanaServerError as e:
                # Handle server-side errors (5xx errors)
                logger.error(f'Failed to create alert "{alert["name"]}". Server error: {e.message}')
            except GrafanaException as e:
                # Catch any other Grafana-related exceptions
                logger.error(
                    f'Failed to create alert "{alert["name"]}". Grafana error: {e.message}'
                )
            except Exception as e:
                # Catch any other exceptions (non-Grafana-related)
                logger.error(f'Failed to create alert "{alert["name"]}". Unexpected error: {e}')

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
                    logger.info(f'Alert rule group "{group_uid}" updated successfully')
            except Exception as e:
                logger.error(f'Failed to update alert rule group "{alert["ruleGroup"]}". {e}')

        if args.out_dir:
            output_dir = f"{args.out_dir}/alerts"
            dump_alert(output_dir=output_dir, alert=alert)

    logger.info("Done building grafana alerts")
