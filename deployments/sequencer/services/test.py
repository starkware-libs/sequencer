

def _get_service_port():
    return [
        port.split('_')[0]
        for port in ["http_server_config.port", "monitoring_endpoint_config.port"]
    ]

print(_get_service_port())