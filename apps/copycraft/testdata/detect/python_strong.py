#!/usr/bin/env python3
import json
from pathlib import Path


class Report:
    def __init__(self, path):
        self.path = Path(path)

    def load(self):
        with self.path.open() as handle:
            return json.load(handle)


if __name__ == "__main__":
    print(Report("demo.json").load())
