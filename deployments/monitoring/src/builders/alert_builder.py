#!/usr/bin/env python3

import argparse
import collections
import json
import os
import sys
from typing import Optional

from common import const
from common.config_overrides import apply_config_overrides as apply_config_overrides_generic
from common.config_overrides import (
    load_config_file,
    validate_config_overrides,
)
from common.grafana10_objects import (
    alert_expression_model_object,
    alert_query_model_object,
    alert_query_object,
    alert_rule_object,
)
from common.logger import get_logger
from grafana_client import GrafanaApi
from tenacity import before_sleep_log, retry, stop_after_attempt, wait_fixed

# Global logger (initialized in alert_builder function)
logger = None


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


def create_rule_group(name: str, interval_sec: int, rules: list[dict[str, any]]) -> dict[str, any]:
    return {"name": name, "interval": interval_sec, "rules": rules}


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


def dump_rule_group(output_dir: str, rule_group: dict[str, any]) -> None:
    group_full_path = f"{output_dir}/{rule_group['name']}.json".lower().replace(" ", "_")
    os.makedirs(output_dir, exist_ok=True)
    with open(group_full_path, "w") as f:
        json.dump(rule_group, f, indent=2)
    logger.info(
        f'[bold white]Rule group[/bold white] "[blue]{rule_group["name"]}[/blue]" [bold white]saved to[/bold white] [dim white]{group_full_path}[/dim white]'
    )


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


def _convert_numeric_strings_in_conditions(conditions: list[dict[str, any]]) -> None:
    """
    Recursively convert numeric strings back to numbers in conditions structure.
    This handles placeholders that were replaced in numeric fields like evaluator.params.
    """
    for condition in conditions:
        if isinstance(condition, dict):
            # Handle evaluator.params and reducer.params in a single loop
            for key in ["evaluator", "reducer"]:
                params = condition.get(key, {}).get("params", [])
                for i, param in enumerate(params):
                    if isinstance(param, str):
                        # Try to convert to float first (handles decimals), then int if whole number
                        try:
                            float_val = float(param)
                            # If it's a whole number, convert to int, otherwise keep as float
                            params[i] = int(float_val) if float_val.is_integer() else float_val
                        except ValueError:
                            # Keep as string if conversion fails
                            pass


def post_process_alert(alert: dict[str, any]) -> dict[str, any]:
    """
    Post-process alert after placeholder replacement.
    Handles alert-specific field conversions (e.g., intervalSec string to int,
    conditions.params numeric strings to numbers).

    Args:
        alert: The alert dictionary

    Returns:
        The alert dictionary with post-processing applied
    """
    # Special handling for intervalSec: if it was a placeholder string that got replaced,
    # try to convert it to int if it's now numeric
    val = alert.get("intervalSec")
    if isinstance(val, str):
        try:
            # Try to convert to int if it's a numeric string
            alert["intervalSec"] = int(val)
        except ValueError:
            # Keep as string if conversion fails
            pass

    # Convert numeric strings in conditions.params back to numbers
    conds = alert.get("conditions")
    if isinstance(conds, list):
        _convert_numeric_strings_in_conditions(conds)

    return alert


def resolve_dev_alerts_file_path(path: str) -> str:
    """
    Resolve a JSON path:
    - If the file exists, return it.
    - Raise an error if neither exists.
    """
    if os.path.isfile(path):
        return path

    raise FileNotFoundError(f"'{path}' does not exist.")


def alert_builder(args: argparse.Namespace):
    global logger
    logger = get_logger(name="alert_builder", debug=args.debug)

    alert_file_path = resolve_dev_alerts_file_path(path=args.dev_alerts_file)

    with open(alert_file_path, "r") as f:
        dev_alerts = json.load(f)

    # Load config overrides if provided
    args_dict = vars(args)
    config = (
        load_config_file(args_dict.get("alert_rules_overrides_config_file"), logger_instance=logger)
        if args_dict.get("alert_rules_overrides_config_file")
        else {}
    )
    if config:
        logger.info(f"Loaded {len(config)} override(s) from alert rules overrides config file")

    if not args.dry_run:
        client = GrafanaApi.from_url(args.grafana_url)
        folder_uid = create_folder_return_uid(client=client, title="Sequencer")
    else:
        folder_uid = args.folder_uid

    # Get config file path for error messages
    alert_rules_overrides_config_file_path = args_dict.get("alert_rules_overrides_config_file", "")

    # Validate all placeholders from all alerts first (before processing any)
    # Always validate, even if config is empty, to catch missing placeholders
    try:
        validate_config_overrides(
            dev_alerts["alerts"],
            config,
            source_json_path=alert_file_path,
            config_override_path=alert_rules_overrides_config_file_path,
            logger_instance=logger,
            item_type_name="alert",
        )
    except ValueError:
        # Error message already printed by validate_config_overrides with Rich formatting
        # Exit cleanly without traceback
        sys.exit(1)

    group_intervals: dict[str, int] = {g["name"]: g["intervalSec"] for g in dev_alerts["groups"]}

    # group_name -> list of alert rules (preserving insertion order within each group)
    groups: dict[str, list[dict[str, any]]] = collections.defaultdict(list)

    for dev_alert in dev_alerts["alerts"]:
        # Apply config overrides to replace placeholders
        if config:
            dev_alert = apply_config_overrides_generic(
                dev_alert,
                config,
                logger_instance=logger,
                item_name=dev_alert["name"],
                post_process=post_process_alert,
            )

        if args.namespace and args.cluster:
            expr = inject_expr_placeholders(
                expr=dev_alert["expr"], namespace=args.namespace, cluster=args.cluster
            )
        else:
            expr = remove_expr_placeholder(expr=dev_alert["expr"])

        group_name = dev_alert["ruleGroup"]
        groups[group_name].append(
            create_alert_rule(
                name=dev_alert["name"],
                title=dev_alert["title"],
                folder_uid=folder_uid,
                rule_group=group_name,
                _for=dev_alert["for"],
                expr=expr,
                conditions=dev_alert["conditions"],
                datasource_uid=args.datasource_uid,
                labels={
                    "og_priority": dev_alert["severity"],
                    "observer_applicable": dev_alert["observer_applicable"],
                },
            )
        )

    rule_groups = [
        create_rule_group(
            name=group_name,
            interval_sec=group_intervals[group_name],
            rules=sorted(rules, key=lambda a: a["name"]),
        )
        for group_name, rules in sorted(groups.items())
    ]

    for rule_group in rule_groups:
        if args.debug:
            logger.debug(json.dumps(rule_group))
        if not args.dry_run:
            try:
                update_alert_rule_group(
                    client=client,
                    folder_uid=folder_uid,
                    group_uid=rule_group["name"],
                    alertrule_group=rule_group,
                )
            except Exception as e:
                logger.error(f'Failed to update rule group "{rule_group["name"]}". {e}')

        if args.out_dir:
            output_dir = f"{args.out_dir}/alerts"
            dump_rule_group(output_dir=output_dir, rule_group=rule_group)

    logger.info("Done building grafana alerts")
