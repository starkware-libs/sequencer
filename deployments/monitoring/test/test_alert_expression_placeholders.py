import json
import pathlib

import pytest
from builders.alert_builder import inject_expr_placeholders, remove_expr_placeholder

CLUSTER = "test-cluster"
NAMESPACE = "test-namespace"

# Grafana substitutes these only where a dashboard supplies them as template variables.
DASHBOARD_VARIABLES = ("$cluster", "$namespace")

SHIPPED_ALERTS_PATH = (
    pathlib.Path(__file__).resolve().parents[3]
    / "crates"
    / "apollo_dashboard"
    / "resources"
    / "dev_grafana_alerts.json"
)


def shipped_alerts() -> list[dict]:
    with open(SHIPPED_ALERTS_PATH) as alerts_file:
        return json.load(alerts_file)["alerts"]


def alert_name(alert: dict) -> str:
    return alert["name"]


@pytest.mark.parametrize("alert", shipped_alerts(), ids=alert_name)
def test_injection_leaves_no_dashboard_variable_in_shipped_alert(alert: dict):
    # An alert rule is evaluated without dashboard variables, so a surviving $cluster or $namespace
    # reaches Prometheus as a literal regex, matches no series, and the rule never fires. Rendering
    # every shipped expression guards the substitution against selectors this test does not
    # enumerate, such as any selector carrying matchers beyond cluster and namespace.
    rendered_expr = inject_expr_placeholders(
        expr=alert["expr"], cluster=CLUSTER, namespace=NAMESPACE
    )

    for variable in DASHBOARD_VARIABLES:
        assert (
            variable not in rendered_expr
        ), f"alert '{alert['name']}' still contains {variable} after injection: {rendered_expr}"
    if any(variable in alert["expr"] for variable in DASHBOARD_VARIABLES):
        assert NAMESPACE in rendered_expr
        assert CLUSTER in rendered_expr


@pytest.mark.parametrize("alert", shipped_alerts(), ids=alert_name)
def test_removal_leaves_valid_selector_in_shipped_alert(alert: dict):
    stripped_expr = remove_expr_placeholder(expr=alert["expr"])

    for variable in DASHBOARD_VARIABLES:
        assert (
            variable not in stripped_expr
        ), f"alert '{alert['name']}' still contains {variable} after removal: {stripped_expr}"
    for dangling_separator in ("{,", ",}", ", }"):
        assert dangling_separator not in stripped_expr, (
            f"alert '{alert['name']}' has a dangling '{dangling_separator}' after removal: "
            f"{stripped_expr}"
        )


# (case id, expression, expression after injection, expression after removal)
PLACEHOLDER_CASES = [
    (
        "bare_selector",
        'up{cluster=~"$cluster", namespace=~"$namespace"}',
        'up{namespace="test-namespace", cluster="test-cluster"}',
        "up{}",
    ),
    (
        "one_extra_matcher",
        'exchange_rate_oracle_rate{cluster=~"$cluster", namespace=~"$namespace", '
        'currency_pair="strk_usd"}',
        'exchange_rate_oracle_rate{namespace="test-namespace", cluster="test-cluster", '
        'currency_pair="strk_usd"}',
        'exchange_rate_oracle_rate{currency_pair="strk_usd"}',
    ),
    (
        "two_extra_matchers",
        'http_server_add_tx_latency_bucket{cluster=~"$cluster", namespace=~"$namespace", '
        'le="1.0", scraper="l1_events"}',
        'http_server_add_tx_latency_bucket{namespace="test-namespace", cluster="test-cluster", '
        'le="1.0", scraper="l1_events"}',
        'http_server_add_tx_latency_bucket{le="1.0", scraper="l1_events"}',
    ),
    (
        "two_selectors_one_with_extra_matcher",
        '(mempool_transactions_dropped{cluster=~"$cluster", namespace=~"$namespace", '
        'drop_reason="evicted"}) and on() (is_observer{cluster=~"$cluster", '
        'namespace=~"$namespace"} == 0)',
        '(mempool_transactions_dropped{namespace="test-namespace", cluster="test-cluster", '
        'drop_reason="evicted"}) and on() (is_observer{namespace="test-namespace", '
        'cluster="test-cluster"} == 0)',
        '(mempool_transactions_dropped{drop_reason="evicted"}) and on() (is_observer{} == 0)',
    ),
    (
        "no_placeholder",
        "vector(0)",
        "vector(0)",
        "vector(0)",
    ),
]


@pytest.mark.parametrize(
    "expr, expected_expr",
    [(expr, injected_expr) for _, expr, injected_expr, _ in PLACEHOLDER_CASES],
    ids=[case_id for case_id, _, _, _ in PLACEHOLDER_CASES],
)
def test_inject_expr_placeholders(expr: str, expected_expr: str):
    assert (
        inject_expr_placeholders(expr=expr, cluster=CLUSTER, namespace=NAMESPACE) == expected_expr
    )


@pytest.mark.parametrize(
    "expr, expected_expr",
    [(expr, stripped_expr) for _, expr, _, stripped_expr in PLACEHOLDER_CASES],
    ids=[case_id for case_id, _, _, _ in PLACEHOLDER_CASES],
)
def test_remove_expr_placeholder(expr: str, expected_expr: str):
    assert remove_expr_placeholder(expr=expr) == expected_expr
