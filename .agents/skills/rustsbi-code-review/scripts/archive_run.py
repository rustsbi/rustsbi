#!/usr/bin/env python3
"""archive_run.py - snapshot a run's artifacts into a timestamped subdirectory.

    archive_run.py <output-file>

The run's files stay directly in the run directory as the overwritable
"latest" copy; the timestamped copy preserves the history of each run:

    <dir>/review.md                      <- latest, overwritten by every run
    <dir>/review.md.input.md
    <dir>/review.md.comments.json
    <dir>/<YYYYMMDD-HHMMSS>/review.md    <- this run, frozen
    <dir>/<YYYYMMDD-HHMMSS>/...

so the files directly under the run directory and the files inside the newest
subdirectory are always identical. Nothing is deleted: older stamps accumulate.
"""
import datetime
import os
import re
import shutil
import sys

NL = chr(10)
STAMP_DIR = re.compile(r"^\d{8}-\d{6}(-\d+)?$")


def main():
    if len(sys.argv) != 2:
        sys.stderr.write("usage: archive_run.py <output-file>" + NL)
        sys.exit(2)
    out = os.path.abspath(sys.argv[1])
    run_dir = os.path.dirname(out)

    stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    dest = os.path.join(run_dir, stamp)
    suffix = 2
    while os.path.exists(dest):  # two runs inside one second
        dest = os.path.join(run_dir, "{}-{}".format(stamp, suffix))
        suffix += 1
    os.makedirs(dest)

    count = 0
    for name in sorted(os.listdir(run_dir)):
        path = os.path.join(run_dir, name)
        if os.path.isfile(path):
            shutil.copy2(path, os.path.join(dest, name))
            count += 1
        elif os.path.isdir(path) and not STAMP_DIR.match(name):
            # Auxiliary subdirectories of this run (e.g. a fetched spec snapshot)
            # belong to the archive; timestamped siblings do not.
            shutil.copytree(path, os.path.join(dest, name),
                            ignore=shutil.ignore_patterns())
            count += 1
    print("{}: {} entries archived".format(dest, count))


if __name__ == "__main__":
    main()
