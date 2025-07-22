from src.builders.alert_builder import alert_builder
from src.builders.dashboard_builder import dashboard_builder
from src.common.helpers import arg_parser, get_logger


# TODO(Idan Shamam): Add more logs to dashboard_builder
# TODO(Idan Shamam): Move all Grafana Client code to separate file
# TODO(Idan Shamam): Change alert_builder and dashboard_builder to classes
# TODO(Idan Shamam): Create exception handling for all Grafana Client code in single place
# TODO(Idan Shamam): In DashboardBuilder, create functions for dump dashboards and dev_dashboards.json file load
def main() -> None:
    args = arg_parser()
    logger = get_logger(name="main", debug=args.debug)

    if args.dev_dashboards_file:
        logger.info("Building dashboards...")
        dashboard_builder(args=args)

    if args.dev_alerts_file:
        logger.info("Building alerts...")
        alert_builder(args=args)


if __name__ == "__main__":
    main()
