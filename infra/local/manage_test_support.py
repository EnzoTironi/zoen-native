import importlib.util
import pathlib

SPEC = importlib.util.spec_from_file_location('local_manage', pathlib.Path(__file__).with_name('manage.py'))
manage = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(manage)
