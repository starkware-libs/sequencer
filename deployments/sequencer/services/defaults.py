from services.objects import Probe, HealthCheck

health_check=HealthCheck(
    startup_probe=Probe(port="http", path="/", period_seconds=5, failure_threshold=10, timeout_seconds=5),
    readiness_probe=Probe(port="http", path="/", period_seconds=5, failure_threshold=10, timeout_seconds=5),
    liveness_probe=Probe(port="http", path="/", period_seconds=5, failure_threshold=10, timeout_seconds=5)
)