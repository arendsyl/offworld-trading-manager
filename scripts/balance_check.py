#!/usr/bin/env python3
"""
Economy Balance Checker for Offworld Trading Manager

Given a population of 10k (6.5k active workers), determines whether the
economy can sustain itself — i.e., can workers produce enough consumer
goods to satisfy consumption demand?

Production model: Y = K^0.4 * L^0.6  (Cobb-Douglas)
Each factory produces output.quantity * Y units and consumes input.quantity * Y units per tick.
"""

import json
import math
import os
from collections import defaultdict
from pathlib import Path

# ── Configuration ───────────────────────────────────────────────────────────

POPULATION = 10_000
ACTIVE_WORKERS = 6_500
WAGE = 32
SAVINGS_RATE = 0.22
CONSUMPTION_BUDGET = (1 - SAVINGS_RATE) * WAGE * ACTIVE_WORKERS  # 50700
CLIMATE = "temperate"

# Default capital per tier
CAPITAL_BY_TIER = {0: 50, 1: 30, 2: 20, 3: 10}

# Default prices per tier
PRICE_BY_TIER = {0: 5, 1: 12, 2: 25, 3: 60}

# Cobb-Douglas exponents
ALPHA = 0.4  # capital exponent
BETA = 0.6  # labor exponent

# ── Data Loading ────────────────────────────────────────────────────────────

SCRIPT_DIR = Path(__file__).resolve().parent
PROJECT_DIR = SCRIPT_DIR.parent

with open(PROJECT_DIR / "data" / "factories.json") as f:
    FACTORIES = json.load(f)

with open(PROJECT_DIR / "data" / "consumptions.json") as f:
    CONSUMPTIONS = json.load(f)

CONSUMPTION_PROFILE = CONSUMPTIONS[CLIMATE]

# ── Build Lookup Tables ─────────────────────────────────────────────────────

# Factory by id
factory_by_id = {f["id"]: f for f in FACTORIES}

# Which factory produces which good (good -> list of factory ids)
producers_of = defaultdict(list)
for f in FACTORIES:
    for out in f["outputs"]:
        producers_of[out["good"]].append(f["id"])

# Which factories consume a good as input (good -> list of factory ids)
consumers_of = defaultdict(list)
for f in FACTORIES:
    for inp in f["inputs"]:
        consumers_of[inp["good"]].append(f["id"])

# All goods
all_goods = set()
for f in FACTORIES:
    for out in f["outputs"]:
        all_goods.add(out["good"])
    for inp in f["inputs"]:
        all_goods.add(inp["good"])

# Consumer goods (goods that appear in the consumption profile)
consumer_goods = set(CONSUMPTION_PROFILE.keys())

# Intermediate goods: produced by factories and consumed by other factories
intermediate_goods = set()
for f in FACTORIES:
    for inp in f["inputs"]:
        intermediate_goods.add(inp["good"])


def cobb_douglas(K, L):
    """Production function: Y = K^alpha * L^beta"""
    if K <= 0 or L <= 0:
        return 0.0
    return (K**ALPHA) * (L**BETA)


def factory_output_per_tick(factory_id, L):
    """
    Compute how many units of each output a factory produces per tick,
    and how many units of each input it consumes, given L workers assigned.
    """
    f = factory_by_id[factory_id]
    K = CAPITAL_BY_TIER[f["tier"]]
    Y = cobb_douglas(K, L)
    outputs = {out["good"]: out["quantity"] * Y for out in f["outputs"]}
    inputs = {inp["good"]: inp["quantity"] * Y for inp in f["inputs"]}
    return Y, outputs, inputs


def labor_for_output(factory_id, target_Y):
    """
    Given a target Y (production multiplier), compute the labor L needed.
    Y = K^alpha * L^beta  =>  L = (Y / K^alpha)^(1/beta)
    """
    f = factory_by_id[factory_id]
    K = CAPITAL_BY_TIER[f["tier"]]
    K_part = K**ALPHA
    if K_part <= 0:
        return float("inf")
    L = (target_Y / K_part) ** (1.0 / BETA)
    return L


# ── Step 1: Compute Consumption Demand ──────────────────────────────────────

print("=" * 80)
print("OFFWORLD TRADING MANAGER — ECONOMY BALANCE CHECK")
print("=" * 80)
print(f"\nPopulation:          {POPULATION:>10,}")
print(f"Active workers:      {ACTIVE_WORKERS:>10,}")
print(f"Wage:                {WAGE:>10}")
print(f"Savings rate:        {SAVINGS_RATE:>10.0%}")
print(f"Consumption budget:  {CONSUMPTION_BUDGET:>10,.0f}")
print(f"Climate:             {CLIMATE:>10}")
print()

# Consumption demand: quantity_g = budget_share_g * total_budget / price_g
# We need to figure out prices. We use the tier of the factory that produces
# each consumer good as its price.
good_price = {}
for good in all_goods:
    # Find the factory that produces this good and use its tier price
    prods = producers_of.get(good, [])
    if prods:
        tier = factory_by_id[prods[0]]["tier"]
        good_price[good] = PRICE_BY_TIER[tier]
    else:
        # Raw good not produced by any factory (shouldn't happen for consumer goods)
        good_price[good] = 5  # default

print("-" * 80)
print("STEP 1: CONSUMPTION DEMAND (temperate climate)")
print("-" * 80)
print(f"{'Good':<30} {'Budget %':>8} {'Price':>6} {'Demand (qty)':>14}")
print("-" * 60)

consumption_demand = {}
total_share = 0.0
for good, share in sorted(CONSUMPTION_PROFILE.items(), key=lambda x: -x[1]):
    price = good_price.get(good, 25)
    demand_qty = share * CONSUMPTION_BUDGET / price
    consumption_demand[good] = demand_qty
    total_share += share
    print(f"{good:<30} {share:>7.0%} {price:>6} {demand_qty:>14.1f}")

print(f"{'TOTAL':<30} {total_share:>7.0%}")
print()

# ── Step 2: Supply Chain Graph ──────────────────────────────────────────────

print("-" * 80)
print("STEP 2: SUPPLY CHAIN GRAPH")
print("-" * 80)

# For each consumer good, trace back through the supply chain to find all
# upstream factories and the total goods needed.


def trace_supply_chain(good, qty_needed, visited=None):
    """
    Recursively trace the supply chain for `good`.
    Returns a dict: factory_id -> required Y (production multiplier)
    Also returns the total labor needed.
    """
    if visited is None:
        visited = set()

    result = {}  # factory_id -> required Y
    prods = producers_of.get(good, [])

    if not prods:
        # Raw resource with no producer — this shouldn't happen in a closed economy
        return result

    # Pick the first (or only) producer
    factory_id = prods[0]
    f = factory_by_id[factory_id]

    # How much Y do we need from this factory to produce qty_needed of this good?
    # output_qty = out_quantity * Y  =>  Y = output_qty / out_quantity
    out_quantity = None
    for out in f["outputs"]:
        if out["good"] == good:
            out_quantity = out["quantity"]
            break

    if out_quantity is None or out_quantity == 0:
        return result

    required_Y = qty_needed / out_quantity

    # Accumulate (may already have some Y required from other paths)
    if factory_id in result:
        result[factory_id] += required_Y
    else:
        result[factory_id] = required_Y

    # Now recurse into inputs
    for inp in f["inputs"]:
        input_good = inp["good"]
        input_qty_needed = inp["quantity"] * required_Y

        # Recurse
        upstream = trace_supply_chain(input_good, input_qty_needed, visited)
        for fid, y in upstream.items():
            if fid in result:
                result[fid] += y
            else:
                result[fid] = y

    return result


# For each consumer good, compute the full supply chain labor requirement
total_factory_Y_needed = defaultdict(float)  # factory_id -> total Y needed

consumer_good_details = {}

for good in sorted(consumption_demand.keys()):
    qty = consumption_demand[good]
    chain = trace_supply_chain(good, qty)
    consumer_good_details[good] = chain

    for fid, y in chain.items():
        total_factory_Y_needed[fid] += y

# Print the supply chain for a few representative goods
for good in sorted(consumption_demand.keys()):
    chain = consumer_good_details[good]
    if chain:
        prods = producers_of.get(good, [])
        if prods:
            prod_name = factory_by_id[prods[0]]["name"]
        else:
            prod_name = "???"
        print(f"\n  {good} (demand={consumption_demand[good]:.1f}) <- {prod_name}")
        for fid, y in sorted(chain.items(), key=lambda x: -x[1]):
            f = factory_by_id[fid]
            L_needed = labor_for_output(fid, y)
            print(f"    {f['name']:<40} Y={y:>8.2f}  L={L_needed:>8.1f}")

print()

# ── Step 3: Total Labor Requirements ───────────────────────────────────────

print("-" * 80)
print("STEP 3: TOTAL LABOR REQUIREMENTS PER FACTORY")
print("-" * 80)
print(
    f"{'Factory':<40} {'Tier':>4} {'Y needed':>10} {'L needed':>10} {'% of workers':>12}"
)
print("-" * 78)

total_labor_needed = 0.0
factory_labor = {}

for fid in sorted(
    total_factory_Y_needed.keys(), key=lambda x: -total_factory_Y_needed[x]
):
    y = total_factory_Y_needed[fid]
    f = factory_by_id[fid]
    L = labor_for_output(fid, y)
    factory_labor[fid] = L
    total_labor_needed += L
    pct = L / ACTIVE_WORKERS * 100
    print(f"{f['name']:<40} {f['tier']:>4} {y:>10.2f} {L:>10.1f} {pct:>11.1f}%")

print("-" * 78)
print(
    f"{'TOTAL':<40} {'':>4} {'':>10} {total_labor_needed:>10.1f} {total_labor_needed / ACTIVE_WORKERS * 100:>11.1f}%"
)
print()

# ── Step 4: Per Consumer Good Labor Breakdown ───────────────────────────────

print("-" * 80)
print("STEP 4: LABOR NEEDED PER CONSUMER GOOD (including upstream)")
print("-" * 80)
print(
    f"{'Consumer Good':<30} {'Demand':>8} {'Total L':>10} {'% of workers':>12} {'Status':>12}"
)
print("-" * 74)

good_labor = {}
bottlenecks = []

for good in sorted(consumption_demand.keys(), key=lambda g: -consumption_demand[g]):
    chain = consumer_good_details[good]
    total_L_for_good = 0.0
    for fid, y in chain.items():
        L = labor_for_output(fid, y)
        total_L_for_good += L
    good_labor[good] = total_L_for_good
    pct = total_L_for_good / ACTIVE_WORKERS * 100
    status = "OK" if pct < 15 else "HEAVY"
    if pct > 20:
        status = "BOTTLENECK"
        bottlenecks.append(good)
    print(
        f"{good:<30} {consumption_demand[good]:>8.1f} {total_L_for_good:>10.1f} {pct:>11.1f}% {status:>12}"
    )

total_good_labor = sum(good_labor.values())
print("-" * 74)
print(
    f"{'SUM (with overlap)':<30} {'':>8} {total_good_labor:>10.1f} {total_good_labor / ACTIVE_WORKERS * 100:>11.1f}%"
)
print(
    f"{'ACTUAL TOTAL (no overlap)':<30} {'':>8} {total_labor_needed:>10.1f} {total_labor_needed / ACTIVE_WORKERS * 100:>11.1f}%"
)
print()

# ── Step 5: Verdict ─────────────────────────────────────────────────────────

print("=" * 80)
print("VERDICT")
print("=" * 80)

surplus = ACTIVE_WORKERS - total_labor_needed
if surplus >= 0:
    print(f"\n  ECONOMY IS SUSTAINABLE")
    print(f"  Labor available:  {ACTIVE_WORKERS:>8,}")
    print(f"  Labor required:   {total_labor_needed:>8,.0f}")
    print(
        f"  Surplus labor:    {surplus:>8,.0f} ({surplus / ACTIVE_WORKERS * 100:.1f}%)"
    )
    print(f"  (Surplus workers can go to investment goods, infrastructure, etc.)")
else:
    deficit = -surplus
    print(f"\n  ECONOMY CANNOT SUSTAIN ITSELF")
    print(f"  Labor available:  {ACTIVE_WORKERS:>8,}")
    print(f"  Labor required:   {total_labor_needed:>8,.0f}")
    print(
        f"  Labor DEFICIT:    {deficit:>8,.0f} ({deficit / ACTIVE_WORKERS * 100:.1f}%)"
    )
    print(
        f"  Workers would need to be {total_labor_needed / ACTIVE_WORKERS:.1f}x more productive"
    )

print()

# ── Step 6: Bottleneck Factory Multiplier Suggestions ───────────────────────

print("-" * 80)
print("STEP 6: SUGGESTED OUTPUT QUANTITY MULTIPLIERS")
print("-" * 80)
print()
print("To bring total labor down to 80% of available (leaving 20% for investment),")
print(f"target total labor = {ACTIVE_WORKERS * 0.80:.0f}")
print()

if total_labor_needed <= ACTIVE_WORKERS * 0.80:
    print("No adjustments needed — economy already fits within 80% of labor.")
else:
    # The overall scaling factor needed
    target_labor = ACTIVE_WORKERS * 0.80
    # Labor scales as Y^(1/beta) * (1/K^(alpha/beta))
    # If we multiply all output quantities by m, then Y_needed decreases by m,
    # and L_needed decreases by m^(1/beta) = m^(1/0.6) = m^1.667
    # So we need: total_labor * m^(-1/beta) = target_labor
    # => m = (total_labor / target_labor)^beta

    # But it's better to target specific factories. Let's find which factories
    # use the most labor and suggest multipliers for them.

    print(
        f"{'Factory':<40} {'Tier':>4} {'Cur Output':>10} {'Cur L':>10} {'Suggested mult':>15} {'New L':>10}"
    )
    print("-" * 92)

    # Sort factories by labor usage
    sorted_factories = sorted(factory_labor.items(), key=lambda x: -x[1])

    # Strategy: for each bottleneck factory, compute what multiplier on its
    # output_quantity would halve its labor requirement.
    # If output_quantity is multiplied by m, then Y_needed = demand / (out_qty * m)
    # decreases by factor m, and L = (Y_needed / K^alpha)^(1/beta) decreases by m^(1/beta)

    overall_ratio = total_labor_needed / target_labor
    # We want to reduce labor by this ratio overall.
    # Target: multiply output quantities so that L decreases proportionally.

    for fid, L in sorted_factories[:20]:
        f = factory_by_id[fid]
        # Current output quantities
        out_strs = ", ".join(f"{o['good']}:{o['quantity']}" for o in f["outputs"])

        # What multiplier on output would reduce this factory's labor by overall_ratio?
        # L_new = L / overall_ratio
        # L = (Y / K^a)^(1/b), so L_new = (Y_new / K^a)^(1/b)
        # Y_new = Y / m (where m is output multiplier)
        # L_new = (Y/(m * K^a))^(1/b) = L / m^(1/b)
        # We want L_new = L / overall_ratio
        # => m^(1/b) = overall_ratio => m = overall_ratio^b
        mult = overall_ratio**BETA  # same for all, but let's show it
        new_L = L / overall_ratio

        print(
            f"{f['name']:<40} {f['tier']:>4} {out_strs:>10} {L:>10.1f} {mult:>14.2f}x {new_L:>10.1f}"
        )

    print()
    print(f"  Overall labor reduction ratio needed: {overall_ratio:.2f}x")
    print(f"  Uniform output multiplier to achieve this: {overall_ratio**BETA:.2f}x")
    print(
        f"  (i.e., multiply ALL factory output quantities by {overall_ratio**BETA:.2f})"
    )
    print()

    # Also provide per-tier suggestions
    print("  Per-tier breakdown of labor:")
    for tier in range(4):
        tier_labor = sum(
            L for fid, L in factory_labor.items() if factory_by_id[fid]["tier"] == tier
        )
        print(
            f"    Tier {tier}: {tier_labor:>8.0f} workers ({tier_labor / ACTIVE_WORKERS * 100:.1f}%)"
        )
    print()

    # Targeted suggestion: only adjust the most labor-hungry factories
    print("  TARGETED SUGGESTIONS (top 10 labor-hungry factories):")
    print(f"  {'Factory':<40} {'Cur qty':>8} {'Suggested qty':>14} {'Labor saved':>12}")
    print("  " + "-" * 76)

    for fid, L in sorted_factories[:10]:
        f = factory_by_id[fid]
        for out in f["outputs"]:
            cur_qty = out["quantity"]
            # Multiplier to halve this factory's labor:
            # L_new = L / 2, so m^(1/b) = 2, m = 2^b = 2^0.6 = 1.516
            suggested_mult = 2.0**BETA
            new_qty = math.ceil(cur_qty * suggested_mult)
            saved = L - L / 2
            print(
                f"  {f['name']:<35} ({out['good']}) {cur_qty:>5} -> {new_qty:>5} {saved:>12.0f}"
            )

print()

# ── Step 7: Sensitivity Analysis — Capital Sweep ────────────────────────────

print("-" * 80)
print("STEP 7: SENSITIVITY ANALYSIS — WHAT IF CAPITAL IS LOWER?")
print("-" * 80)
print()
print("How does total labor change as capital decreases from default?")
print("(Simulates early-game when factories are undercapitalized)")
print()
print(
    f"{'Capital mult':<14} {'T0 K':>5} {'T1 K':>5} {'T2 K':>5} {'T3 K':>5} {'Total L':>10} {'% workers':>10} {'Sustainable?':>13}"
)
print("-" * 70)

for cap_mult_pct in [100, 75, 50, 25, 15, 10, 5, 3, 2, 1]:
    cap_mult = cap_mult_pct / 100.0
    test_capital = {t: max(K * cap_mult, 0.01) for t, K in CAPITAL_BY_TIER.items()}

    # Recompute labor for all factories with this capital
    test_total_L = 0.0
    for fid, y_needed in total_factory_Y_needed.items():
        f = factory_by_id[fid]
        K = test_capital[f["tier"]]
        K_part = K**ALPHA
        L = (y_needed / K_part) ** (1.0 / BETA)
        test_total_L += L

    pct = test_total_L / ACTIVE_WORKERS * 100
    ok = "YES" if test_total_L <= ACTIVE_WORKERS else "NO"
    print(
        f"{cap_mult_pct:>10}%  {test_capital[0]:>5.1f} {test_capital[1]:>5.1f} {test_capital[2]:>5.1f} {test_capital[3]:>5.1f} {test_total_L:>10.0f} {pct:>9.1f}% {ok:>13}"
    )

print()

# ── Step 8: Break-even Capital ──────────────────────────────────────────────

print("-" * 80)
print("STEP 8: BREAK-EVEN CAPITAL (minimum K multiplier for sustainability)")
print("-" * 80)
print()

# Binary search for the minimum capital multiplier where total_L <= ACTIVE_WORKERS
lo, hi = 0.001, 1.0
for _ in range(50):
    mid = (lo + hi) / 2
    test_capital = {t: max(K * mid, 0.001) for t, K in CAPITAL_BY_TIER.items()}
    test_total_L = 0.0
    for fid, y_needed in total_factory_Y_needed.items():
        f = factory_by_id[fid]
        K = test_capital[f["tier"]]
        K_part = K**ALPHA
        L = (y_needed / K_part) ** (1.0 / BETA)
        test_total_L += L
    if test_total_L <= ACTIVE_WORKERS:
        hi = mid
    else:
        lo = mid

break_even = (lo + hi) / 2
print(f"  Break-even capital multiplier: {break_even * 100:.1f}% of default")
print(
    f"  At this level: T0={CAPITAL_BY_TIER[0] * break_even:.1f}, T1={CAPITAL_BY_TIER[1] * break_even:.1f}, T2={CAPITAL_BY_TIER[2] * break_even:.1f}, T3={CAPITAL_BY_TIER[3] * break_even:.1f}"
)
print()

# Verify
test_capital = {t: max(K * break_even, 0.001) for t, K in CAPITAL_BY_TIER.items()}
test_total_L = 0.0
for fid, y_needed in total_factory_Y_needed.items():
    f = factory_by_id[fid]
    K = test_capital[f["tier"]]
    K_part = K**ALPHA
    L = (y_needed / K_part) ** (1.0 / BETA)
    test_total_L += L
print(
    f"  Verification: total L at break-even = {test_total_L:.0f} (available = {ACTIVE_WORKERS})"
)

# ── Step 9: Per-tier labor share ────────────────────────────────────────────
print()
print("-" * 80)
print("STEP 9: LABOR BREAKDOWN BY TIER")
print("-" * 80)
print()

for tier in range(4):
    tier_factories = [
        (fid, factory_labor[fid])
        for fid in factory_labor
        if factory_by_id[fid]["tier"] == tier
    ]
    tier_total = sum(L for _, L in tier_factories)
    pct = tier_total / total_labor_needed * 100 if total_labor_needed > 0 else 0
    print(
        f"  Tier {tier} (K={CAPITAL_BY_TIER[tier]:>3}, price={PRICE_BY_TIER[tier]:>3}): {tier_total:>8.1f} workers ({pct:>5.1f}% of required labor)"
    )
    # Top 3 in this tier
    tier_factories.sort(key=lambda x: -x[1])
    for fid, L in tier_factories[:3]:
        print(f"    {factory_by_id[fid]['name']:<40} L={L:>8.1f}")
    if len(tier_factories) > 3:
        rest = sum(L for _, L in tier_factories[3:])
        print(
            f"    {'(remaining ' + str(len(tier_factories) - 3) + ' factories)':<40} L={rest:>8.1f}"
        )

print()
print("=" * 80)
print("END OF ANALYSIS")
print("=" * 80)
