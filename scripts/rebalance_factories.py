#!/usr/bin/env python3
"""
Rebalance factory output/input quantities so that ~75% of workers are needed
at equilibrium to satisfy consumption demand.

Scales all quantities by a uniform factor, preserving VA ratios.
"""

import json
import math
from collections import defaultdict
from pathlib import Path

# ── Configuration ─────────────────────────────────────────────────────────
TARGET_EMPLOYMENT = 0.75  # 75% of active workers
ACTIVE_WORKERS = 6_500
WAGE = 10
SAVINGS_RATE = 0.22
CONSUMPTION_BUDGET = (1 - SAVINGS_RATE) * WAGE * ACTIVE_WORKERS
CLIMATE = "temperate"
CAPITAL_BY_TIER = {0: 50, 1: 30, 2: 20, 3: 10}
PRICE_BY_TIER = {0: 5, 1: 12, 2: 25, 3: 60}
ALPHA = 0.4
BETA = 0.6

# ── Load Data ─────────────────────────────────────────────────────────────
PROJECT_DIR = Path(__file__).resolve().parent.parent

with open(PROJECT_DIR / "data" / "factories.json") as f:
    factories = json.load(f)
with open(PROJECT_DIR / "data" / "consumptions.json") as f:
    consumptions = json.load(f)

profile = consumptions[CLIMATE]
factory_by_id = {f["id"]: f for f in factories}
producers_of = defaultdict(list)
for f in factories:
    for out in f["outputs"]:
        producers_of[out["good"]].append(f["id"])

all_goods = set()
for f in factories:
    for out in f["outputs"]:
        all_goods.add(out["good"])

good_price = {}
for good in all_goods:
    prods = producers_of.get(good, [])
    if prods:
        good_price[good] = PRICE_BY_TIER[factory_by_id[prods[0]]["tier"]]
    else:
        good_price[good] = 5

# ── Compute Demand ────────────────────────────────────────────────────────
consumption_demand = {}
for good, share in profile.items():
    price = good_price.get(good, 25)
    consumption_demand[good] = share * CONSUMPTION_BUDGET / price

# ── Trace Supply Chain ────────────────────────────────────────────────────
def trace_supply_chain(good, qty_needed):
    result = defaultdict(float)
    prods = producers_of.get(good, [])
    if not prods:
        return result
    fid = prods[0]
    f = factory_by_id[fid]
    out_qty = next((o["quantity"] for o in f["outputs"] if o["good"] == good), None)
    if not out_qty:
        return result
    Y_needed = qty_needed / out_qty
    result[fid] += Y_needed
    for inp in f["inputs"]:
        upstream = trace_supply_chain(inp["good"], inp["quantity"] * Y_needed)
        for uid, y in upstream.items():
            result[uid] += y
    return result

def compute_total_labor(factory_list):
    """Compute total labor needed given current factory quantities."""
    fbi = {f["id"]: f for f in factory_list}
    prod_of = defaultdict(list)
    for f in factory_list:
        for out in f["outputs"]:
            prod_of[out["good"]].append(f["id"])

    def trace(good, qty):
        result = defaultdict(float)
        prods = prod_of.get(good, [])
        if not prods:
            return result
        fid = prods[0]
        f = fbi[fid]
        out_qty = next((o["quantity"] for o in f["outputs"] if o["good"] == good), None)
        if not out_qty:
            return result
        Y_needed = qty / out_qty
        result[fid] += Y_needed
        for inp in f["inputs"]:
            upstream = trace(inp["good"], inp["quantity"] * Y_needed)
            for uid, y in upstream.items():
                result[uid] += y
        return result

    total_Y = defaultdict(float)
    for good, qty in consumption_demand.items():
        chain = trace(good, qty)
        for fid, y in chain.items():
            total_Y[fid] += y

    total_L = 0.0
    per_factory = {}
    for fid, y in total_Y.items():
        f = fbi[fid]
        K = CAPITAL_BY_TIER[f["tier"]]
        L = (y / (K ** ALPHA)) ** (1.0 / BETA)
        total_L += L
        per_factory[fid] = L
    return total_L, per_factory

# ── Current State ─────────────────────────────────────────────────────────
current_L, current_per_factory = compute_total_labor(factories)
target_L = TARGET_EMPLOYMENT * ACTIVE_WORKERS

print(f"Current total labor needed: {current_L:.0f} ({current_L/ACTIVE_WORKERS*100:.1f}%)")
print(f"Target labor: {target_L:.0f} ({TARGET_EMPLOYMENT*100:.0f}%)")
print()

# ── Compute Scaling Factor ────────────────────────────────────────────────
# L ∝ 1/qty^(1/β). If we multiply qty by f, L scales by 1/f^(1/β).
# We want: current_L / f^(1/β) = target_L
# f^(1/β) = current_L / target_L
# f = (current_L / target_L)^β

ratio = current_L / target_L  # < 1, meaning we need MORE labor
# To get more labor, we need to DECREASE quantities (f < 1)
# f = ratio^β won't work because ratio < 1 and we want to decrease...
# Actually: current_L * (1/f)^(1/β) = target_L  where f is the scale factor on quantities
# Wait, if we DIVIDE quantities by d:
# new_L = current_L * d^(1/β)
# d^(1/β) = target_L / current_L
# d = (target_L / current_L)^β

d = (target_L / current_L) ** BETA
print(f"Quantity divisor: {d:.3f}")
print(f"i.e., multiply all quantities by {1/d:.3f}")
print()

# But we need integers >= 1, so let's binary search for the right divisor
# that gives us closest to target after rounding

def apply_divisor(factory_list, divisor):
    """Scale all input/output quantities by 1/divisor, round to int, min 1."""
    new_factories = []
    for f in factory_list:
        nf = dict(f)
        nf["outputs"] = []
        for o in f["outputs"]:
            new_qty = max(1, round(o["quantity"] / divisor))
            nf["outputs"].append({"good": o["good"], "quantity": new_qty})
        nf["inputs"] = []
        for i in f["inputs"]:
            new_qty = max(1, round(i["quantity"] / divisor))
            nf["inputs"].append({"good": i["good"], "quantity": new_qty})
        # Keep build_cost unchanged
        new_factories.append(nf)
    return new_factories

# Binary search for best divisor
best_div = d
best_diff = float('inf')

for trial in [x * 0.1 for x in range(10, 100)]:
    scaled = apply_divisor(factories, trial)
    L, _ = compute_total_labor(scaled)
    diff = abs(L - target_L)
    if diff < best_diff:
        best_diff = diff
        best_div = trial

# Fine-tune
for trial_x10 in range(int(best_div * 10) - 10, int(best_div * 10) + 10):
    trial = trial_x10 / 10.0
    if trial <= 0:
        continue
    scaled = apply_divisor(factories, trial)
    L, _ = compute_total_labor(scaled)
    diff = abs(L - target_L)
    if diff < best_diff:
        best_diff = diff
        best_div = trial

print(f"Best divisor (after rounding): {best_div:.1f}")
scaled_factories = apply_divisor(factories, best_div)
new_L, new_per_factory = compute_total_labor(scaled_factories)
print(f"New total labor needed: {new_L:.0f} ({new_L/ACTIVE_WORKERS*100:.1f}%)")
print()

# ── Show Changes ──────────────────────────────────────────────────────────
print(f"{'Factory':<40} {'Old out':>8} {'New out':>8} {'Old in':>8} {'New in':>8} {'Old L':>8} {'New L':>8}")
print("-" * 100)

for orig, scaled in zip(factories, scaled_factories):
    old_out = ", ".join(f"{o['quantity']}" for o in orig["outputs"])
    new_out = ", ".join(f"{o['quantity']}" for o in scaled["outputs"])
    old_in = ", ".join(f"{i['quantity']}" for i in orig["inputs"]) or "-"
    new_in = ", ".join(f"{i['quantity']}" for i in scaled["inputs"]) or "-"
    old_L = current_per_factory.get(orig["id"], 0)
    new_l = new_per_factory.get(orig["id"], 0)
    print(f"{orig['name']:<40} {old_out:>8} {new_out:>8} {old_in:>8} {new_in:>8} {old_L:>8.1f} {new_l:>8.1f}")

print()

# ── VA check at equilibrium prices ───────────────────────────────────────
print("VA CHECK (at tier-based prices):")
print(f"{'Factory':<40} {'Old VA':>8} {'New VA':>8} {'Change':>8}")
print("-" * 66)

for orig, scaled in zip(factories, scaled_factories):
    old_va = (sum(o["quantity"] * good_price.get(o["good"], 1) for o in orig["outputs"])
              - sum(i["quantity"] * good_price.get(i["good"], 1) for i in orig["inputs"]))
    new_va = (sum(o["quantity"] * good_price.get(o["good"], 1) for o in scaled["outputs"])
              - sum(i["quantity"] * good_price.get(i["good"], 1) for i in scaled["inputs"]))
    if old_va != 0:
        change = (new_va - old_va) / abs(old_va) * 100
    else:
        change = 0
    # Only show if VA changed significantly or went negative
    flag = " *** NEGATIVE ***" if new_va < 0 else ""
    print(f"{orig['name']:<40} {old_va:>8.0f} {new_va:>8.0f} {change:>+7.0f}%{flag}")

print()

# ── Write or preview ──────────────────────────────────────────────────────
import sys
if "--write" in sys.argv:
    out_path = PROJECT_DIR / "data" / "factories.json"
    with open(out_path, "w") as f:
        json.dump(scaled_factories, f, indent=2)
        f.write("\n")
    print(f"Written to {out_path}")
else:
    print("Dry run — pass --write to save to factories.json")
