<img src="https://r2cdn.perplexity.ai/pplx-full-logo-primary-dark%402x.png" style="height:64px;margin-right:32px"/>

## Overview

This macro model is designed to plug into a larger simulation where each planet is an isolated economy trading via bilateral import/export contracts. The core focus is on **physical stocks of goods**, not just money. Production uses a Cobb‑Douglas function in capital $K$ and labor $L$, constrained by the availability of input goods. Prices are adaptive signals driven by the ratio of desired demand to available supply.

***

## Core Concepts

### 1. Physical stocks

**Concept**
For each planet $p$ and good $g$, you track a **physical stock** $S_{p,g}(t)$ at the beginning of tick $t$. This stock is the only source of inputs for production and the only source of goods available for consumption, investment, and exports during that tick.

**Why it matters**
This enforces hard physical constraints: if a newly colonized planet has money but zero stock of steel, it cannot build factories. It also cleanly separates *when* goods exist (after production is credited) from *when* they can be used.

**Tick behavior (high level)**

At tick $t$:

- Start-of-tick stock:
$S_g(t)$ = stock carried over from tick $t-1$.
- During the tick, **production and demand** consume from these stocks.
- At the very end of the tick, the **new production** is credited to stocks, becoming available only at tick $t+1$.

***

### 2. Production: Cobb‑Douglas + input constraints

**Concept**
Each sector $i$ on a planet produces some good $g$ using a Cobb‑Douglas production function in capital and labor, with a Leontief-style input requirement on intermediate goods.

- Potential output (technology side):

$$
Y^{\text{pot}}_{i}(t) = A_i \, K_i(t)^{\alpha_i} \, L_i(t)^{1-\alpha_i}
$$
- Input constraint (materials side): given technical coefficients $a_{j,i}$ (units of good $j$ per unit of output of $i$):

$$
Y^{\text{io}}_{i}(t) = \min_j \frac{S_j(t)}{a_{j,i}} \quad \text{(only starting stocks count)}
$$
- Effective output:

$$
Y_{i}(t) = \min\bigl(Y^{\text{pot}}_{i}(t),\, Y^{\text{io}}_{i}(t)\bigr)
$$

Whenever sector $i$ produces, it **immediately debits** the required inputs from current stocks:

- For each input good $j$:
$S_j(t) \leftarrow S_j(t) - a_{j,i} \, Y_i(t)$.

The produced output remains “in transit” during the tick and is only added to its output good’s stock at the end:

- At end of tick:
$S_g(t+1) \leftarrow S_g(t+1) + \sum_{i \in \text{producers of } g} Y_i(t)$.

**Why it matters**

- Using **only beginning-of-tick stocks** to constrain production avoids simultaneity and dependency cycles between sectors.
- End-of-tick crediting of production introduces a natural one-tick delay: goods produced now become usable later, as in real supply chains.

***

### 3. Desired vs. actual consumption (and investment)

**Concept**
For each good $g$ at tick $t$, agents express **desired demand**:

- Desired consumption: $D^{C}_g(t)$
- Desired investment: $D^{I}_g(t)$
- Desired export: $D^{X}_g(t)$

Total desired demand:

$$
D_g(t) = D^{C}_g(t) + D^{I}_g(t) + D^{X}_g(t)
$$

But what actually happens is limited by physical stocks and imports.

- Available supply for domestic use during tick $t$:

$$
O_g(t) = S_g(t) + \text{Imports}_g(t) - \text{Exports}_g^{\text{committed}}(t)
$$

(where committed exports are contracts you choose to honor this tick).
- **Realized sales / actual usage**:

$$
V_g(t) = \min\bigl(D_g(t),\, O_g(t)\bigr)
$$

**Why it matters**
The distinction between **desired** and **actual** demand creates:

- Real physical shortages: desired demand can exceed what is physically possible.
- A clean way to define rationing and measure unmet needs.
- A foundation for capital growth based on **realized** investment, not planned investment.

***

### 4. Rationing under domestic priority

**Concept**
When there is a shortage ($D_g(t) > O_g(t)$), domestic uses (consumption and investment) are prioritized over exports. Within domestic uses, the remaining stock is allocated **proportionally to desired demand**.

**Domestic priority rule**

1. Compute domestic demand:
$D^{\text{dom}}_g(t) = D^{C}_g(t) + D^{I}_g(t)$.
2. If $O_g(t) \ge D^{\text{dom}}_g(t)$:
    - Domestic demand is fully satisfied.
    - Remaining supply can go to exports up to:

$$
V^{X}_g(t) = \min\bigl(D^{X}_g(t),\, O_g(t) - D^{\text{dom}}_g(t)\bigr)
$$
3. If $O_g(t) < D^{\text{dom}}_g(t)$: **domestic rationing**:
    - Total actually used domestically:

$$
V^{\text{dom}}_g(t) = O_g(t)
$$
    - Proportional allocation:

$$
V^{C}_g(t) = V^{\text{dom}}_g(t)\,\frac{D^{C}_g(t)}{D^{\text{dom}}_g(t)}, 
\quad
V^{I}_g(t) = V^{\text{dom}}_g(t)\,\frac{D^{I}_g(t)}{D^{\text{dom}}_g(t)}
$$
    - Exports get **zero** in this case:
$V^{X}_g(t) = 0$.
4. Total actual usage:

$$
V_g(t) = V^{C}_g(t) + V^{I}_g(t) + V^{X}_g(t)
$$

**Why it matters**

- This encodes a **policy / institutional choice**: planets will sacrifice exports before starving their own consumption and investment.
- Proportional rationing is simple, fair, and numerically stable; no arbitrary priority order among domestic uses is needed unless you want it.

***

### 5. Stock update within a tick

Given the above, the stock evolution for good $g$ is:

1. Start with $S_g(t)$.
2. During the tick:
    - Production consumes inputs (already applied to other goods’ stocks).
    - Domestic demand and exports consume from $S_g(t)$ and imports.
3. Let $O_g(t)$ be the effective available supply for usage before production is credited:

$$
O_g(t) = S_g(t) + \text{Imports}_g(t) - \text{Exports}_g^{\text{real}}(t)
$$
4. Usage (realized demand) is $V_g(t)$ as defined above.
5. End-of-tick stock after usage but before crediting production:

$$
S^{\text{pre-prod}}_g(t+1) = O_g(t) - V_g(t)
$$
6. End-of-tick stock after crediting production:

$$
S_g(t+1) = S^{\text{pre-prod}}_g(t+1) + Q^{\text{prod}}_g(t)
$$

where $Q^{\text{prod}}_g(t)$ is the total effective output of $g$ during tick $t$.

**Why it matters**
This ensures:

- No negative stocks (you always cap usage at what’s available).
- One-tick production delay.
- Clear accounting: every unit produced or consumed is tracked as a change in $S_g$.

***

### 6. Price formation from supply–demand tension

**Concept**
Price is a **lagged signal** of market tension, not an instantaneous equilibrium. Each tick, the price adjusts smoothly based on the ratio of desired demand to available supply.

Define the tension ratio for good $g$:

$$
R_g(t) = \frac{D_g(t)}{O_g(t)}
$$

Then update price as:

$$
P_g(t) = P_g(t-1)\,\bigl[\alpha \cdot R_g(t) + \beta\bigr]
$$

with:

- $\alpha \in (0,1)$ a responsiveness parameter (e.g. $0.1$–$0.2$),
- $\beta = 1 - \alpha$.

Interpretation:

- If $R_g(t) = 1$: demand matches available supply → $P_g(t) = P_g(t-1)$.
- If $R_g(t) > 1$: desired demand exceeds available supply → price rises.
- If $R_g(t) < 1$: available supply exceeds desired demand → price falls.

You can optionally clamp price changes per tick (e.g. limit price to ±50% change per tick) to avoid extreme volatility.

**Why it matters**

- Pricing becomes **adaptive** and smooth, not jumpy.
- The price encapsulates both stock levels and flows (since $O(t)$ includes stock + imports, and $D(t)$ includes all uses).
- Agents can use these prices in their decision rules (e.g. consumption, investment) without needing to see the full stock–flow structure.

***

### 7. Realized vs. desired investment and capital growth

**Concept**
Capital $K_i(t)$ for sector $i$ evolves based on **realized** investment, not desired investment. Let $V^{I}_i(t)$ be the actual physical investment goods allocated to sector $i$ at tick $t$.

A simple capital accumulation rule:

$$
K_i(t+1) = K_i(t) + \delta_i \, V^{I}_i(t)
$$

with $\delta_i$ a capital conversion efficiency (e.g. 0.8–1.2).

**Why it matters**

- Shortages in investment goods directly limit capital growth.
- A sector in persistent shortage will see its capital grow more slowly, constraining future Cobb‑Douglas output and creating rich long‑term dynamics.

***

## Worked Examples

Assume a single good $g$ (e.g. “metal”), one planet, and one tick length. Let:

- Start-of-tick stock $S(t) = 100$.
- No imports.
- Exports **not** prioritized over domestic use (domestic priority).
- Price last tick $P(t-1) = 1.0$.
- $\alpha=0.2,\ \beta=0.8$.


### 1. Perfect equilibrium case

**Setup**

- Desired domestic consumption: $D^{C}(t) = 40$.
- Desired domestic investment: $D^{I}(t) = 40$.
- Desired exports: $D^{X}(t) = 20$.
- Domestic demand: $D^{\text{dom}}(t) = 40 + 40 = 80$.
- Total desired demand: $D(t) = 80 + 20 = 100$.

Available supply during tick:

- $O(t) = S(t) = 100$.

**Allocation**

1. Domestic demand vs supply: $O(t) = 100 \ge D^{\text{dom}}(t) = 80$.
Domestic is fully satisfied:
$V^{C}(t) = 40,\ V^{I}(t) = 40$.
2. Remaining supply: $100 - 80 = 20$.
Exports get:
$V^{X}(t) = \min(20, 20) = 20$.
3. Total actual usage:
$V(t) = 40 + 40 + 20 = 100$.

End-of-tick stock before production:

- $S^{\text{pre-prod}}(t+1) = O(t) - V(t) = 100 - 100 = 0$.

Suppose production during tick is $Q^{\text{prod}}(t) = 100$.
End-of-tick stock:

- $S(t+1) = 0 + 100 = 100$.

**Price**

Tension ratio:

- $R(t) = D(t) / O(t) = 100 / 100 = 1$.

Price update:

- $P(t) = 1.0 \cdot [0.2 \cdot 1 + 0.8] = 1.0$.

**Interpretation**

- Physically: what is consumed and exported is exactly replaced by production. Stocks remain constant.
- Price: stable, indicating no persistent excess demand or supply.

***

### 2. Shortage (domestic priority) case

**Setup**

- Start-of-tick stock: $S(t) = 100$.
- Desired domestic consumption: $D^{C}(t) = 60$.
- Desired domestic investment: $D^{I}(t) = 60$.
- Desired exports: $D^{X}(t) = 80$.
- Domestic demand: $D^{\text{dom}}(t) = 60 + 60 = 120$.
- Total desired demand: $D(t) = 120 + 80 = 200$.

Available supply:

- $O(t) = 100$.

**Domestic priority and rationing**

1. $O(t) = 100 < D^{\text{dom}}(t) = 120$.
    - All 100 units go to domestic uses, exports get 0.
    - Domestic rationing:
$V^{\text{dom}}(t) = 100$.
Fraction for consumption: $60/120 = 0.5$.
Fraction for investment: $60/120 = 0.5$.
    - Actual usage:
$V^{C}(t) = 100 \times 0.5 = 50$.
$V^{I}(t) = 100 \times 0.5 = 50$.
$V^{X}(t) = 0$.
2. Total actual usage:
$V(t) = 50 + 50 + 0 = 100$.

End-of-tick stock before production:

- $S^{\text{pre-prod}}(t+1) = 100 - 100 = 0$.

Suppose production during tick is constrained by input shortages and yields only $Q^{\text{prod}}(t) = 80$.
End-of-tick stock:

- $S(t+1) = 0 + 80 = 80$.

**Price**

Tension ratio:

- $R(t) = D(t) / O(t) = 200 / 100 = 2$.

Price update:

- $P(t) = 1.0 \cdot [0.2 \cdot 2 + 0.8] = 1.0 \cdot (0.4 + 0.8) = 1.2$.

**Interpretation**

- **Physical impact**:
    - Consumption and investment each receive only 50 out of 60 desired units.
    - Exports are fully cut to protect domestic uses.
    - Stocks shrink to 80, so the system is under strain.
- **Price impact**:
    - Price increases by 20%.
    - Next tick, higher prices should reduce desired demand and attract investment (in the broader model), gradually easing the shortage.

***

### 3. Overproduction / oversupply case

**Setup**

- Start-of-tick stock: $S(t) = 100$.
- Desired domestic consumption: $D^{C}(t) = 20$.
- Desired domestic investment: $D^{I}(t) = 10$.
- Desired exports: $D^{X}(t) = 10$.
- Domestic demand: $D^{\text{dom}}(t) = 30$.
- Total desired demand: $D(t) = 40$.

Available supply:

- $O(t) = 100$.

**Allocation**

1. Domestic demand first:
$O(t) = 100 \ge D^{\text{dom}}(t) = 30$.
Domestic demand fully satisfied:
$V^{C}(t) = 20,\ V^{I}(t) = 10$.
2. Remaining supply: $100 - 30 = 70$.
Exports get:
$V^{X}(t) = \min(10, 70) = 10$.
3. Total actual usage:
$V(t) = 20 + 10 + 10 = 40$.

End-of-tick stock before production:

- $S^{\text{pre-prod}}(t+1) = 100 - 40 = 60$.

Suppose production is high due to previous investment and yields $Q^{\text{prod}}(t) = 100$.
End-of-tick stock:

- $S(t+1) = 60 + 100 = 160$.

**Price**

Tension ratio:

- $R(t) = D(t) / O(t) = 40 / 100 = 0.4$.

Price update:

- $P(t) = 1.0 \cdot [0.2 \cdot 0.4 + 0.8] = 1.0 \cdot (0.08 + 0.8) = 0.88$.

**Interpretation**

- **Physical impact**:
    - All desired demand is satisfied easily.
    - Stocks grow from 100 to 160, indicating oversupply.
- **Price impact**:
    - Price falls by 12%.
    - Cheaper prices will tend to increase consumption and possibly reduce incentives to expand production further, nudging the system back toward balance.

***

This structure gives you:

- Hard physical realism via explicit stocks and one-tick production delays.
- A clear distinction between desired and realized usage.
- A simple, robust rationing mechanism with domestic priority over exports.
- A smooth, interpretable price formation rule that reacts to supply–demand tension.

It should integrate cleanly into a larger SF simulation, with additional layers (financial sector, savings/investment allocation, interplanetary contracts) building on top of these core mechanics.

