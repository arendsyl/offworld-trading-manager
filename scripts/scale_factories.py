#!/usr/bin/env python3
"""Scale factory input/output quantities by a multiplier and set production_cycle."""

import json
from pathlib import Path

MULTIPLIER = 30
PRODUCTION_CYCLE = 100

path = Path(__file__).resolve().parent.parent / "data" / "factories.json"

with open(path) as f:
    factories = json.load(f)

for factory in factories:
    for o in factory["outputs"]:
        o["quantity"] = o["quantity"] * MULTIPLIER
    for i in factory["inputs"]:
        i["quantity"] = i["quantity"] * MULTIPLIER
    # build_cost stays unchanged
    factory["production_cycle"] = PRODUCTION_CYCLE

with open(path, "w") as f:
    json.dump(factories, f, indent=2)
    f.write("\n")

print(f"Scaled {len(factories)} factories: quantities x{MULTIPLIER}, production_cycle={PRODUCTION_CYCLE}")
