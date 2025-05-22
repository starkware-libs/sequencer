empty_dashboard = {
    "annotations": {
        "list": [
            {
                "builtIn": 1,
                "datasource": {"type": "grafana", "uid": "-- Grafana --"},
                "enable": True,
                "hide": True,
                "iconColor": "rgba(0, 211, 255, 1)",
                "name": "Annotations & Alerts",
                "type": "dashboard",
            }
        ]
    },
    "editable": True,
    "fiscalYearStartMonth": 0,
    "graphTooltip": 0,
    "links": [],
    "liveNow": False,
    "panels": [],
    "refresh": "5s",
    "schemaVersion": 38,
    "style": "dark",
    "tags": [],
    "templating": {"list": []},
    "time": {"from": "now-5m", "to": "now"},
    "timepicker": {},
    "timezone": "",
    "title": "New dashboard",
    "version": 0,
    "weekStart": "",
}

templating_object = {
    "list": [
        {
            "allValue": "",
            "current": {"selected": True, "text": [], "value": []},
            "datasource": {"type": "prometheus", "uid": "Prometheus"},
            "definition": "label_values(batcher_proposal_started,namespace)",
            "hide": 0,
            "includeAll": True,
            "multi": True,
            "name": "namespace",
            "title": "Namespace",
            "options": [],
            "query": {
                "qryType": 1,
                "query": "label_values(batcher_proposal_started,namespace)",
                "refId": "PrometheusVariableQueryEditor-VariableQuery",
            },
            "refresh": 1,
            "regex": "",
            "skipUrlSync": False,
            "sort": 0,
            "type": "query",
        },
        {
            "allValue": "",
            "current": {"selected": True, "text": [], "value": []},
            "datasource": {"type": "prometheus", "uid": "Prometheus"},
            "definition": "label_values(batcher_proposal_started,cluster)",
            "hide": 0,
            "includeAll": True,
            "multi": True,
            "name": "cluster",
            "title": "Cluster",
            "options": [],
            "query": {
                "qryType": 1,
                "query": "label_values(batcher_proposal_started,cluster)",
                "refId": "PrometheusVariableQueryEditor-VariableQuery",
            },
            "refresh": 1,
            "regex": "",
            "skipUrlSync": False,
            "sort": 0,
            "type": "query",
        },
    ]
}

row_object = {
    "collapsed": True,
    "gridPos": {"h": 1, "w": 24, "x": 0, "y": 0},
    "id": 1,
    "panels": [],
    "title": "Row title 1",
    "type": "row",
}


alert_query_model_condition_object = {
    "evaluator": {"params": [0], "type": "gt"},
    "operator": {"type": "and"},
    "query": {"params": ["C"]},
    "reducer": {"params": [], "type": "last"},
    "type": "query",
}

alert_expression_model_object = {
    "conditions": [],
    "datasource": {"name": "Expression", "type": "__expr__", "uid": "__expr__"},
    "expression": "A",
    "hide": False,
    "intervalMs": 1000,
    "maxDataPoints": 43200,
    "refId": "B",
    "type": "threshold",
}

alert_query_model_object = {
    "editorMode": "code",
    "instant": True,
    "intervalMs": 1000,
    "legendFormat": "__auto",
    "maxDataPoints": 43200,
    "range": False,
    "refId": "A",
    "expr": "",
}

alert_query_object = {
    "refId": "A",
    "queryType": "",
    "relativeTimeRange": {"from": 600, "to": 0},
    "datasourceUid": "PBFA97CFB590B2093",
    "model": {},
}

alert_rule_object = {
    "name": "",
    "title": "",
    "orgId": 1,
    "condition": "B",
    "data": [],
    "for": "5m",
    "execErrState": "Error",
    "noDataState": "NoData",
    "folderUID": "",
    "ruleGroup": "",
    "annotations": {},
    "labels": {},
    "isPaused": False,
}
