from common.helpers import get_logger, arg_parser
from builders.dashboard_builder import dashboard_builder
from builders.alert_builder import alert_builder


# TODO(Idan Shamam): Add more logs to dashboard_builder
# TODO(Idan Shamam): Move all Grafana Client code to separate file
def main():
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
