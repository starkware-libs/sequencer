import pathlib
import sys

# Puts deployments/monitoring/src on sys.path, so tests import the builder modules by the same
# names the app uses when run as `python -m main` from that directory.
sys.path.insert(0, str(pathlib.Path(__file__).parent / "src"))
