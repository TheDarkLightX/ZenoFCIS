# No free value

Vault borrowing is non-recourse, following the [Liquity V1 liquidation
mechanics](https://github.com/liquity/dev/blob/main/packages/contracts/contracts/TroveManager.sol).
The protocol's only claim on a borrower is the vault's collateral. If its
collateral is worth less than its debt, liquidation removes that collateral
and the vault debt. The borrower keeps the zUSD already borrowed. Offset debt
is burned against Stability Pool deposits; unoffset debt is redistributed to
surviving vaults. A price gap can therefore impose losses on pool depositors
or other vaults. No personal liability beyond collateral is imputed.

For a user, the holdings vector is (AGRS, zUSD, zDEX). Count wallet balances,
owned escrow and current floored realizable claims once, never also their
custody. Count each active vault's effective collateral and composite debt,
including current floored pending redistribution, as zUSD equity:

`equity = max(0, floor(collateral_atoms * committed_price_e8 / 100000000) - composite_debt_atoms)`.

Thus vault equity enters the zUSD component; vault collateral is not also
added to AGRS, and composite debt is not subtracted again. Recorded gas
reserve escrow remains separately counted in zUSD while that vault is active,
as in the escrow-inclusive law. It is conditional escrow, not spendable cash.
Closed vaults contribute no equity or reserve. Surplus, compounded SP
principal, liquidation/reward claims, fee claims and staked zDEX use their
current individual floors; collective remainders are not individual claims.

At fixed committed price and time, compare a user's initial vector with its
final vector less exact newly earned permitted benefits. The adjusted final vector must not have every component at least its initial
amount and one strictly larger. Permitted benefits must name the source
event, payer, asset, recipient and exact amount: keeper compensation, new SP
liquidation gains, funded scheduled rewards, staking/host fees and authorized
external funding. Releasing an already counted claim earns no new benefit.
Borrowing is an exchange of collateral equity for zUSD, not free value.
This law does not guarantee solvency, collateral prices or the market peg.

The permanent liquidation/redemption tests disclose both non-recourse
valuation and the pool's exact shortfall. The sequence check exhausts every
own-command sequence through length two from the 17 declared seed states in
`tests/command_refinement.rs`. This is finite checked through length two;
it is not an arbitrary-length runtime theorem or a coalition result.

A plain price-weighted scalar potential of these floored holdings does not
prove arbitrary lengths: at price 1.5, adding one collateral atom can raise
floored equity by two zUSD atoms while costing only 1.5 atoms of value.
This is no dominance gain, but it refutes scalar one-step monotonicity.
A general proof needs exact floor residuals and an endpoint argument; that
runtime proof remains open. The conditional reserve credit is not a claim
that each vector component can be withdrawn independently.
