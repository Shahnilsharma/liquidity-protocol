
In our vault model, users deposit a certain amount of the underlying asset into the vault, and the vault invests all assets into an external yield-generating lending protocol; the vault tracks the total value of assets managed (`totalAssets`) and the total number of vault share tokens outstanding (`totalShares`). The **price per share** at any moment is calculated as:
[
\text{pricePerShare} = \frac{\text{totalAssets}}{\text{totalShares}}
]
When a user deposits an amount `A`, the number of shares to mint is computed using:
[
\text{sharesToMint} = \frac{A}{\text{pricePerShare}}
]
which ensures the user receives a proportional claim on the vault’s assets. All deposited assets are added to `totalAssets` and `totalShares` increases by `sharesToMint`. As yield accrues in the lending protocol, `totalAssets` grows while `totalShares` remains constant, causing `pricePerShare` to increase; this increase represents the yield earned by the vault and is reflected in the value of each share. When a user redeems `N` shares, the amount of underlying assets returned is:
[
\text{returnAmount} = N \times \text{pricePerShare}
]
so users receive their principal plus their share of the accumulated yield. This share-based accounting ensures correct yield distribution without requiring a separate reward pool — all economics flow through the increasing `pricePerShare` invariant defined by the vault’s total assets and total shares. ([linkedin.com][1])

