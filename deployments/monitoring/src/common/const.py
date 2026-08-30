# The cluster/namespace matchers inside a metric selector's brace group, substituted at
# provisioning time since alert rules carry no dashboard-variable context. Braces are excluded so
# selectors that carry further matchers still match.
# fmt: off
ALERT_RULE_EXPRESSION_PLACEHOLDER = 'cluster=~\"$cluster\", namespace=~\"$namespace\"'
# fmt: on
