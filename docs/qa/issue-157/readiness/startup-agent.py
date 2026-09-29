#!/usr/bin/env python3
"""Real ACP startup failure once, then the existing deterministic local QA peer."""
import pathlib
import runpy
import sys
import time

directory = pathlib.Path(sys.argv[1]).parent
attempt = directory / "startup-attempted"
if not attempt.exists():
    attempt.write_text("first ACP process started\n")
    # Let the native replay type the next draft while this real process connects.
    deadline = time.monotonic() + 120
    while not (directory / "release-startup-failure").exists():
        if time.monotonic() > deadline:
            raise RuntimeError("Native replay did not release startup failure")
        time.sleep(0.02)
    print("Intentional ACP startup failure for recovery QA", file=sys.stderr, flush=True)
    sys.exit(17)
runpy.run_path(str(pathlib.Path(__file__).resolve().parents[1] / "fake-agent.py"), run_name="__main__")
