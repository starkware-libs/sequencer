import os
import yaml

from src.config.merger import merge_configs
from src.cli import argument_parser


def main() -> None:
    args = argument_parser()

    # Get the absolute directory path of main.py
    base_dir = os.path.dirname(os.path.abspath(__file__))

    layout_common_config = os.path.join(base_dir, "configs", "layouts", args.layout, "common.yaml")
    layout_services_config_dir = os.path.join(
        base_dir, "configs", "layouts", args.layout, "services"
    )

    overlay_common_config = None
    overlay_services_config_dir = None
    if args.overlay:
        overlay_common_config = os.path.join(
            base_dir, "configs", "overlays", args.layout, args.overlay, "common.yaml"
        )
        overlay_services_config_dir = os.path.join(
            base_dir, "configs", "overlays", args.layout, args.overlay, "services"
        )

    deployment_config = merge_configs(
        layout_common_config_path=layout_common_config,
        layout_services_config_dir_path=layout_services_config_dir,
        overlay_common_config_path=overlay_common_config,
        overlay_services_config_dir_path=overlay_services_config_dir,
    )

    print("✅ Final config:")
    print(
        yaml.safe_dump(
            deployment_config.model_dump(exclude_none=True), sort_keys=False  # <- convert to dict
        )
    )


if __name__ == "__main__":
    main()
