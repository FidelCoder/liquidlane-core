#!/usr/bin/env python3
"""Consistent online SQLite backup; destination must not already exist."""
import argparse
import os
import sqlite3
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("database", type=Path)
parser.add_argument("destination", type=Path)
args = parser.parse_args()
if not args.database.is_file():
    parser.error("database does not exist")
args.destination.parent.mkdir(parents=True, exist_ok=True)
fd = os.open(args.destination, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
os.close(fd)
with sqlite3.connect(args.database.resolve().as_uri() + "?mode=ro", uri=True) as source:
    with sqlite3.connect(args.destination) as target:
        source.backup(target)
        if target.execute("PRAGMA integrity_check").fetchone()[0] != "ok":
            raise SystemExit("backup integrity check failed")
print(f"Verified SQLite backup: {args.destination}")
