"""Keep helper-owned logs when a verification run requests durable evidence."""
import os
from pathlib import Path
import shutil
import signal


def preserve_logs(root, label):
    destination = os.environ.get('SIGNALTTY_QA_LOG_DIR')
    if destination:
        directory = Path(destination) / label
        directory.mkdir(parents=True, exist_ok=True)
        for log in root.glob('*.log'):
            shutil.copyfile(log, directory / log.name)


def terminate(_signal, _frame):
    raise SystemExit('QA terminated; cleaning up owned processes')


signal.signal(signal.SIGTERM, terminate)
