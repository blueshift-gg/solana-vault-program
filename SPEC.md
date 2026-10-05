# Solana Vault Program — v1 spec

## Problem Statement

Solana has no ERC-4626 equivalent that teams can reuse. Every protocol that needs a
vault writes its own share accounting, its own way of valuing deployed funds, and its
own withdrawal queue. Integrators (lending markets, aggregators, wallets) then have
to support each one separately.

The existing attempts do not fill the gap:

- The Solana Vault Standard is twelve separate unaudited programs, devnet only, and
  mints shares on nominal amounts with no Token-2022 checks.
- The Solana Foundation vault is a fork-me template that dropped its synchronous
  track and lets an authority set NAV with no bounds.
- Voltr has the right shape (core plus CPI adaptors) but a closed-source core,
  unfixed discriminators and unbounded reported values.
- Drift's Trusted class and Voltr's Trustful adaptor both accept a manager-reported
  value with no on-chain limit.

A vault curator today cannot deploy a vault whose safety properties an integrator can
read from one audited program, and a depositor cannot tell how far a manager or a
buggy adapter can move the share price.

## Solution

One program. A vault is an account, not a deployment. It holds one asset mint, issues
one share mint, and is exposed to exactly one thing: one adapter program, the only
place deployed funds are ever handed to. There is one mode. A strategy on chain, a
strategy off chain and a strategy across several protocols are three adapters, not
three kinds of vault.

The program makes eight guarantees, and an integrator can rely on them for every
vault without reading the vault's adapter:

1. **Fixed exposure.** A vault's adapter never changes.
2. **Stored accounting.** Shares are priced only from numbers the program stored. A
   token balance, an adapter's return value and a manager's report are inputs, never
   the price.
3. **Slow up, fast down.** A gain is locked when it is counted and reaches the share
   price evenly over a published unlock period; a loss reaches it at once. So a gain
   is never a step that capital arriving just before it can take. Returning funds to
   idle moves principal only; it never changes the price.
4. **No stale prices.** No deposit or redemption executes on a value older than the
   published maximum age.
5. **Contained adapters.** An adapter can sign only for its own strategy's token
   account, never for idle funds or the share mint.
6. **Exits cannot be switched off.** Not by pause, not by wind-down, and not by a
   withdrawal authority once a ticket is older than the published delay. Whatever
   is in idle, or can be pulled from the adapter, can be claimed at a fresh price. A
   strategy that can no longer be priced holds exits up until the guardian writes
   it off.
7. **Changes wait; brakes don't.** Every configuration change sits out a timelock.
   Pausing, winding down and writing off a loss are instant.
8. **One body of math.** Every value-bearing formula is a pure function in one
   module, and the program computes value nowhere else.

Exits go through one instruction. A redemption idle can cover is paid on the spot;
one it cannot becomes a ticket, fulfilled when funds are there. Wrapping several
protocols is a composite adapter's job, inside the adapter.

## User Stories

### Depositor

1. As a depositor, I want to deposit the asset and receive shares at the current
   stored price, so that I hold a proportional claim on the vault.
2. As a depositor, I want to state the minimum shares I will accept, so that a price
   refresh in the same slot cannot give me fewer than I expected.
3. As a depositor, I want deposits to fail when the vault's value is stale, so that I
   never enter at an outdated price.
4. As a depositor, I want rounding to always favour the vault, so that no one can
   extract value from me through repeated small operations.
5. As a depositor, I want a donation to the vault's token account to have no effect on
   the share price, so that the first-depositor inflation attack does not work.
6. As a depositor, I want the share price to rise no faster than a published maximum
   rate, so that a manager or adapter cannot mint itself fees or dilute me with a
   fabricated gain.
7. As a depositor, I want losses reflected in the share price immediately, so that
   later exits do not leave me holding them.

### Redeemer

8. As a redeemer, I want to request a redemption by escrowing my shares in a ticket,
   so that my exit is recorded on-chain.
9. As a redeemer, I want to create and fulfil my ticket in one transaction when the
   vault is liquid, so that withdrawal is instant.
10. As a redeemer in an onchain vault, I want fulfilment to pull the shortfall from
    the adapter itself, so that I do not depend on the manager to free funds.
11. As a redeemer, I want my ticket priced at fulfilment, so that I bear the same
    gains and losses as every other holder until I am paid.
12. As a redeemer, I want a ticket to be partially fulfilled when idle funds are
    short, so that I receive what is available now.
13. As a redeemer, I want to cancel a ticket and get my shares back, so that my shares
    cannot be held hostage in escrow.
14. As a redeemer in a vault with a withdrawal authority, I want anyone to be able to
    fulfil my ticket once it is older than the published delay, so that the authority
    cannot hold idle funds against me indefinitely.
15. As a redeemer, I want exits to keep working while the vault is paused or wound
    down, so that neither can trap my funds.
16. As a redeemer, I want the ticket's rent returned to me when it closes, so that
    exiting costs only transaction fees.

### Manager

17. As a manager, I want to allocate idle funds to the vault's adapter, so that the
    vault earns yield.
18. As a manager, I want to deallocate funds back to idle, so that I can rebalance or
    prepare for redemptions.
19. As a manager, I want to pass opaque data through to the adapter, so that the
    adapter can take the arguments its protocols need.
20. As a manager, I want allocation capped at a published debt cap, so that depositors
    know my maximum exposure.
21. As a manager of funds held off chain, I want an adapter that sends them to one
    custody account named at setup, so that my operating key decides when funds
    move but never where.
22. As a manager of funds held off chain, I want to report their value by signing a
    statement off chain that anyone can deliver, so that I never need to send a
    transaction to keep the vault priced.

### Authorities

23. As a deposit authority, I want deposits to need my permit, signed off chain, so
    that the vault admits only whom I have cleared.
24. As a withdrawal authority, I want fulfilments of young tickets to need my permit,
    so that I can settle redemptions in the order I choose.
25. As a sponsor, I want to pay the rent of a vault or a ticket without being its
    owner, and to get a ticket's rent back when it closes.

### Owner

26. As an owner, I want to create a vault with its asset, adapter,
    roles, limits, fees and timelock, so that it is fully described at creation.
27. As an owner, I want the vault's adapter to be fixed for its lifetime, so that
    depositors know its exposure can never change.
28. As an owner, I want to submit a configuration change and execute it after the
    timelock, so that depositors have time to exit before risk increases.
29. As an owner, I want risk-reducing changes to apply immediately, so that I can act
    quickly in an incident.
30. As an owner, I want to hand ownership over in two steps, so that a mistyped
    address cannot lock the vault.
31. As an owner, I want performance and management fees accrued as shares, so that
    fees never require selling assets.
32. As an owner, I want to put the vault into wind-down permanently, so that it takes
    no new deposits or allocations while every holder can still exit.
33. As a fee recipient, I want to turn fee shares into the asset through the same
    redeem path as every holder, so that fees never get priority over redeemers.

### Guardian

34. As a guardian, I want to pause deposits and allocations, so that I can stop new
    exposure during an incident.
35. As a guardian, I want to write a loss off when part of the strategy is gone and
    no report will say so, so that the share price reflects it and holders can still
    exit.

### Integrator

36. As an integrator, I want to read share conversions and deposit limits through
    return data, so that my program can price shares by CPI.
37. As an integrator, I want deposits to make no adapter CPI, so that I have call
    depth left to call the vault from my own program.
38. As an integrator, I want to read from the vault account whether exits are
    permissionless, so that I can decide whether shares are acceptable collateral.
39. As an integrator, I want one refresh instruction to prepend before any priced
    operation, so that integrating any vault follows the same recipe.
40. As an integrator, I want supporting a vault to mean supporting its one adapter, so
    that account discovery stays tractable.

### Adapter author

41. As an adapter author, I want a three-instruction interface with fixed
    discriminators, so that my program works with every vault.
42. As an adapter author, I want to implement the interface from either Anchor or
    Pinocchio, so that the standard does not dictate my framework.
43. As an adapter author, I want to write a composite adapter that calls several
    protocols, so that one vault can hold a combined position.
44. As an adapter author, I want to publish my extra accounts in my IDL and a client
    resolver, so that I am not constrained by a static on-chain account list.
45. As an adapter author, I want the vault to sign with a PDA that controls only my
    strategy's token account, so that my program is never handed authority over idle
    funds.

### Indexer and verifier

46. As an indexer, I want one event per instruction with a fixed layout, so that I can
    reconstruct every vault's history.
47. As an indexer, I want events that other programs cannot forge, so that my data is
    trustworthy.
48. As a verifier, I want every value-bearing formula isolated in one pure module, so
    that I can property-test it now and formally verify it later.
49. As a verifier, I want the program to call those same functions for all value
    arithmetic, so that a proof covers what actually runs.

## Implementation Decisions

### Stack and shape

- Follow the solana-gacha reference: Pinocchio, `no_std`, no heap allocator,
  one-byte instruction discriminators, alignment-1 account layouts overlaid on raw
  bytes, and one struct per instruction that validates accounts and data in
  `TryFrom` and executes in `process`.
- Instructions are grouped by role, with discriminator ranges per role: owner,
  guardian, manager, user, and permissionless.
- Workspace: the program, a shared `no_std` core crate (layouts, constants, errors,
  math), and a tests crate. The core crate is consumed by the program and by tests
  now, and by clients later.
- Pin the same Pinocchio version family as the reference; confirm exact versions at
  implementation time, not from memory.

### Accounts

- **Vault.** Asset mint, share mint, idle token account, strategy token account,
  share escrow account, stored `idle`, stored `debt`, stored `total_shares`, accrued
  fee shares, owner and pending owner, the adapter's program id (fixed at creation),
  strategy authority, active configuration,
  pending configuration with its executable time, timelock duration (fixed at
  creation), last report slot, rate clock, fee clock, unrecovered loss, status
  (active, paused or wind-down), bumps and version.
- **Configuration** (one struct, held twice as active and pending): manager,
  guardian, fee recipient, optional deposit authority, optional withdrawal authority, debt cap, deposit cap,
  maximum age in slots, fulfil delay, unlock period, performance fee, management fee.
- **Ticket.** Vault, owner, payer, nonce, remaining shares, creation time, bump and version. The
  address derives from the vault, the owner and a client-chosen id, so one owner can
  hold several tickets and concurrent requests never contend.
- The vault's total assets are `idle + debt − locked`, always from stored values. The idle
  token account's balance is a claim that `simulate` adopts, never the price itself.
- `total_shares` is stored in the vault and includes escrowed and accrued fee shares.

### Instructions

- **Owner:** create vault; submit configuration; execute configuration; transfer
  ownership; accept ownership; wind down.
- **Guardian:** pause and unpause; write off a loss.
- **Manager:** allocate; deallocate.
- **User:** deposit; request redeem; cancel redeem.
- **Permissionless or gated:** simulate; fulfil; collect fees.
- **Views:** convert to shares; convert to assets; max deposit. Each returns a
  little-endian `u64` through return data. They form the read interface: their
  discriminators are eight-byte namespaced hashes, so any vault program can
  implement the same three and an integrator prices shares the same way everywhere.

### One path through the adapter

- `simulate`: anyone may call it. The program CPIs the adapter, unsigned, and reads
  its claimed value from return data, checking the returning program id and an exact
  eight-byte length.
- `allocate`: the program moves funds to the strategy token account and CPIs the
  adapter's deposit, signed by the strategy authority.
- `deallocate`: the program CPIs the adapter's withdraw and sweeps the strategy
  account into idle.
- `fulfil`: when idle is short and the adapter accounts are passed, the program CPIs
  the adapter's withdraw for the shortfall before paying.

### Permits and payers

- An authority never co-signs a transaction. It signs a permit off chain: an Ed25519
  signature over a message binding the program, the kind of permit, the vault, the
  subject and an expiry. Whoever holds the permit presents it in instruction data,
  and the program verifies it with brine-ed25519.
- A deposit permit's subject is the depositor; it is required when the vault has a
  deposit authority. A fulfil permit's subject is the ticket; it is required while a
  ticket is younger than the fulfil delay and the vault has a withdrawal authority.
- A permit is reusable until it expires and is good for nothing but its subject, kind
  and vault.
- Every instruction that creates an account takes a payer separate from the authority
  it acts for. A ticket records its payer and returns the rent there when it closes.

### Accounting rules

- Only `simulate` reprices. It takes two claims, what the adapter says the strategy is
  worth and what the idle account actually holds, stores both in full, and records
  the report slot. A loss lowers the price at once. A gain is added to `locked`.
- Locked gains unlock along a straight line over the unlock period, as in Yearn V3.
  When a new gain joins gains still locked, the whole unlocks over the average of
  what each had left, weighted by size, so a small gain cannot restart the wait for a
  large one. A loss is taken from locked gains first, since they were never in the
  price.
- Unlocking happens before anything that prices shares: every deposit, redemption,
  fulfilment and repricing. It depends only on time, so it cannot be hurried by a
  deposit or reset by anyone.
- Tokens sent straight to the idle account, as an incentive for holders for example,
  are a gain like any other: counted by the next `simulate`, locked, and unlocked over
  the period. The same goes for funds that come back after a write-off.
- A deposit is priced against everything the vault has counted, locked gains
  included; a redemption against what has unlocked. Someone entering while gains are
  locked pays for them in full and gets them back as they unlock, so entering takes
  nothing from the holders the gains belong to. Leaving before they unlock leaves
  one's share of them behind, which is what makes capital that arrives for a gain and
  leaves right after pay for the visit. Vesting alone was not enough: a review showed
  that after a write-off, when locked gains are large next to the price, a small
  deposit bought most of the recovery.
- A vault that holds nothing while shares exist takes no deposit.
- What a transfer fee withholds when principal moves is a loss, taken like any other.
- `allocate` and `deallocate` are principal movements. They change `idle` and `debt`
  by the measured token balance change and never by a value the adapter returns. The
  adapter's deposit and withdraw therefore return nothing; only its simulate returns
  a value.
- Funds return to idle as principal, up to `debt`, so total assets never change on
  the way back. What the strategy account holds beyond `debt`, a gain not reported
  yet or a donation, stays there until the next `simulate`, which adds that balance to
  the adapter's claimed value. From then on it is debt, and can be brought home.
- `deposit` and `fulfil` require the last report to be within the maximum age in
  slots, where zero means the current slot. A vault with no debt waives this check,
  since there is nothing a report could change.
- Share conversion uses one virtual asset and one virtual share, and every rounding
  direction favours the vault.
- Fees accrue as shares counted in `total_shares`; `collect fees` mints them to the
  fee recipient.
- The management fee is charged on total assets over elapsed time, before every
  report, deposit and fulfilment. A deposit carries the fee clock forward in
  proportion, so new money never pays for time before it arrived.
- The performance fee is charged as gains unlock, on what unlocks beyond earlier
  losses. Recovering a loss earns no fee. The loss carried is an amount of
  assets, not a per-share mark, so holders leaving or joining after a loss make it
  approximate, in the holders' favour.
- Fees are paid in shares only. A recipient who wants the asset redeems through a
  ticket like any holder.
- Fee ceilings are constants taken from Morpho Vault V2: performance fee at most 50%,
  management fee at most 5% per year.
  Each constant carries a test that states its defining property.

### Redemption

- `request redeem` uses idle first. If idle covers the whole request at a fresh
  price, the shares are burned and the assets paid in that instruction, and no ticket
  is created. Otherwise the shares move into a vault-owned escrow token account under
  a ticket. A request waits as a ticket whenever it cannot be paid in full right now:
  idle is short, the price is stale, or the vault has a withdrawal authority whose
  permit a new ticket would need.
- `fulfil` prices the ticket at the current stored price, pays up to what idle allows,
  burns the corresponding escrowed shares, and closes the ticket when it is empty.
  Payment goes to a token account owned by the ticket's owner.
- Tickets are unordered; any ticket may be fulfilled.
- If a withdrawal authority is set, only it may fulfil a ticket younger than the
  fulfil delay. After the delay, or when no authority is set, anyone may.
- `cancel redeem` returns the remaining shares and closes the ticket. There is no
  penalty because pricing happens at fulfilment.
- Redemption carries no slippage bound.
- A fulfilment that would pay nothing is rejected, so shares are never burned for
  zero assets. Such a ticket stays open until the price recovers or it is cancelled.
- The fulfil delay has a ceiling of 90 days, taken from Drift Vaults' redeem period,
  so a withdrawal authority's priority window always ends.

### Roles and timelock

- Every configuration change, including the manager, the guardian and the withdrawal
  authority, goes through submit then execute after the timelock. One pending
  configuration exists at a time; submitting replaces it. Executing restarts the fee
  clock, so a new fee never applies to time before it.
- A vault is active, paused or in wind-down. Pause is the guardian's and is
  reversible. Wind-down is the owner's, applies immediately and is permanent.
- Pause and wind-down both block deposit and allocate, and nothing else. Simulate,
  request, cancel, fulfil, deallocate and collect fees work in every status.
- Write-off is loss recognition: the guardian sets `debt` down to what the strategy
  is still worth, and the loss is carried like a reported one. It changes no status.
  Written down to zero, the vault has no debt and needs no report, so holders can
  exit against idle when an adapter no longer answers.

### Adapter interface

- Three instructions: simulate, deposit, withdraw.
- Discriminators are eight bytes, the leading bytes of the SHA-256 of a namespaced
  string per instruction, so adapters written in Anchor or Pinocchio can both
  implement them and they cannot collide with an adapter's own instructions. A test
  re-derives each constant from its string.
- Fixed account prefix: the vault's strategy authority, the strategy token account,
  the asset mint and the token program. The authority signs deposit and withdraw
  only; simulate is unsigned. It is writable when the caller passes it writable,
  because some protocols require that of a signer. Everything after is
  adapter-specific and forwarded with its writable flag and never as a signer. One call carries at most 32 accounts and 256 bytes of
  opaque data.
- The strategy authority is a PDA of the vault that owns only the strategy token
  account. The vault PDA that controls idle funds and the share mint is never passed
  to an adapter as a signer.
- The program stores no adapter account list. Adapters publish their accounts in
  their IDL and ship a client resolver for state-dependent ones.
- Composite adapters call protocols directly. An adapter cannot call back into the
  vault program, because the runtime forbids that reentrancy.

### Tokens

- The client creates the share mint and the vault's token accounts. The share mint
  may be legacy SPL Token or Token-2022, and creation checks two things about it:
  the vault PDA is its mint authority, and it has no transfer hook program. Its
  decimals, its extensions, a freeze authority and any existing supply are its
  creator's business, except that it is not the asset mint itself. Each token account
  has the expected mint and owner, no delegate, no close authority and no memo
  requirement, since the program's own transfers carry no memo.
- A ticket records the shares that actually arrive in escrow, so a share mint that
  withholds a transfer fee cannot leave a ticket claiming more than escrow holds.
- The vault address derives from the share mint alone, so the mint, the accounts and
  the vault are created in one transaction.
- The asset mint may be legacy SPL Token or Token-2022. The only thing rejected is a
  transfer hook with a program set: it runs foreign code inside every transfer,
  needs accounts the vault does not carry, and costs a level of call depth. A hook
  extension with no program passes.
- Every other Token-2022 extension is the manager's call before using the token.
  A freeze authority, a pausable mint or a permanent delegate is an issuer power the
  program cannot bound; an issuer that later sets a hook program freezes the vault's
  transfers the same way.
- Amounts arriving in idle or at the strategy are counted by balance change, never
  assumed, so a transfer fee cannot make the stored count exceed what is held. A
  depositor is credited what arrives; a redeemer bears the fee on the payout.

### Events

- Every state-changing instruction emits one event through a self-CPI signed by an
  event authority PDA, as in the reference repo. The wire layout is the event
  discriminator, the instruction discriminator, then the fields in order.

### Math module

- All value-bearing arithmetic lives in a math module inside the core crate, as pure
  functions over integers with no account types, no syscalls and no Pinocchio
  imports: share conversion in both directions, the unlocking of gains, performance and
  management fee shares, and the fulfilment split between assets paid and shares
  burned.
- Handlers perform no value arithmetic of their own; they call these functions. This
  is what makes a later proof cover the shipped program.
- Intermediate products use `u128`; every function returns an error on overflow and
  never wraps.

## Testing Decisions

Tests are end to end. A test runs the vault program, a shipped adapter and the token
programs together under Mollusk, drives them through instructions the way a client
would, and asserts on account state, token balances, return data and error codes. It
never reaches into a handler.

There is one tests crate, with a harness per adapter that sets a vault up behind it.
There is no fake adapter: a vault's adapter is trusted with what the vault hands it,
so a hostile one is outside what the tests cover.

1. **The vault behind the custody adapter.** This is the main suite. The test plays
   the people around the vault: the oracle signing prices, the custodian sending
   funds back, the authorities signing permits. It covers:
   - the lifecycle: deposit, allocate, reprice, instant and partial redemption,
     deallocation, cancel, the views;
   - permits for deposits and fulfilments, and the fulfil delay expiring;
   - a gain unlocking over the period and a loss landing at once, a second gain
     joining the first, capital entering while gains are locked taking none of them,
     a recovery after a write-off going to the holders who took the loss,
     performance and management fees, new money
     paying no fee for time before it, a recovered or written-off loss earning none;
   - tokens sent straight to the vault dripping in, and tokens taken counting as a
     loss at once;
   - caps and slippage; pause and wind-down never blocking exits; a silent oracle
     and a write-off;
   - the timelock, two-step ownership, and which accounts and tokens a vault
     accepts, including Token-2022 with a real transfer fee.
2. **The custody adapter's own rules:** units at the oracle's price, a price surviving
   a flow, expiry and replay, a plain-transfer return, more coming back than the book
   holds, oracle rotation.
3. **The vault on Jupiter Lend itself:** the real lending and liquidity programs and
   a mainnet snapshot of the USDC market. Supply, a month of interest, deallocation,
   and an exit that pulls its shortfall from the market, plus the accounts the
   adapter pins.

Two further checks are not scenarios: the transcribed constants are re-derived from
their sources, and the generated clients are checked against the program.

The value formulas have no unit tests. The scenarios exercise them through the
program, and they are proved in Lean for all inputs, on a translation of the shipped
Rust (`verification/`).

Gates before every commit: formatting, clippy with warnings denied, and the full test
suite.

## Out of Scope

- Client SDKs in Rust and TypeScript, and an IDL for the vault program.
- Further adapters (Kamino, a composite).
- An allocator program that holds shares of several vaults.
- Multi-asset vaults.
- Fees paid in the asset token. Evaluated and declined: it needs a fee liability
  that competes with redeemers for idle funds and a second branch on the value path,
  and a share recipient can already redeem.
- Compute-unit benches.
- An instant "lower caps" instruction. Evaluated and removed: a pause is the
  emergency brake, and a cap changes through the timelock like the rest of the
  configuration.
- FIFO or any ordering of tickets.
- Asynchronous deposits.
- An on-chain adapter account list or account resolution.
- Native SOL as an asset without wrapping.
- Recording whether an adapter program is upgradeable.
- Formal verification itself; this spec only arranges the code so it is possible.
- Deployment, audit and freezing the program.

## Further Notes

- A vault behind the custody adapter is custody trust. The adapter fixes where funds
  go and the vault bounds exposure and reported gains, and whatever reaches idle can
  be claimed. Nothing can force a custodian to approve funds back.
- The custody adapter replaced a native offchain mode, and keeps its book the way a
  fund does. What is off chain is counted in units; funds going out buy units at the
  current price and funds coming back redeem them. An oracle named at setup signs the
  price of one unit off chain, anyone delivers it, and the position is worth units
  times price. Flows and price being separate numbers, a price signed before a flow
  is still right after it. The oracle computes its price as the off-chain value
  divided by the units stored on chain, so the two books cannot drift apart.
- In that adapter funds leave to one custody account fixed at setup, and come back by
  a plain transfer to a return account the adapter owns; the next deallocation or
  fulfilment settles what arrived against units and carries it to idle. Nothing rests
  in the adapter: the vault's idle account is the only liquidity buffer.
- A price is good until its expiry. Past it the adapter stops answering, so the vault
  goes stale and prices nothing until the oracle signs again. A new oracle, named by
  the vault's owner, takes over after the vault's own timelock.
- The first real adapter is Jupiter Lend. Writing it changed two things: the
  strategy authority is forwarded writable, which the lending program requires of
  its signer, and a vault using it must make its strategy account the strategy
  authority's associated token account, which the liquidity program requires of a
  withdrawal recipient. The interface otherwise held: four prefix accounts, fourteen
  of the adapter's own, and the full call depth on a fulfilment that pulls from the
  market (vault, adapter, lending, liquidity, token).
- The Jupiter Lend adapter values a position at the exchange price the lending
  account last stored, which is exact or slightly low and never high. The lending
  program's permissionless `update_rate`, run in the same transaction, makes it exact.
- Call depth is the tight budget. Manager to vault to adapter to protocol to token
  program is four CPIs deep, which is the limit; rejecting transfer hooks on the asset
  is what keeps that path viable. The proposal to raise the limit was not confirmed
  active on mainnet.
- A vault whose adapter stops responding cannot price deposits or redemptions until
  the guardian writes it off. That is the cost of a fixed, single adapter. The same
  holds for a custody vault whose manager stops signing reports.
- The timelock and the maximum age have no floor or ceiling. They are fixed or
  timelocked, and visible in the vault account; an integrator reads them.
- Two adversarial reviews have run. The first, on the first implementation, found
  five issues. The second, five reviewers on the current design, reproduced 21. Its
  main results: a stranger could drain funds returned to the custody adapter (now
  signed by the vault only); a banked rate allowance let flash capital take a gain
  or be wiped for free (replaced by gains unlocking over a period); a recovery after
  a full write-off was stranded or taken by a dust deposit (now locked like any gain,
  with deposits refused while shares are worth nothing); a fulfil permit could be
  reused on a new ticket (now bound to a nonce). All are fixed and covered by tests.
- A holder who knows a loss has been signed but not yet delivered can leave at the
  old price. That is inherent to off-chain valuation. A vault valued that way should
  set a withdrawal authority and a fulfil delay, which also turns off redemption on
  the spot.

- The Token-2022 extension list and the runtime limits were gathered from research
  reports; re-check them against primary sources when implementing.
- Prior art behind each mechanism: stored accounting (Yearn V3, SPL Stake Pool),
  virtual shares (OpenZeppelin ERC-4626), rate-limited gains (Morpho Vault V2),
  same-slot refresh (Kamino), return-data adapters and a per-strategy signer (Voltr),
  request then fulfil at fulfilment price (ERC-7540, Lagoon), authority-gated
  settlement with a fallback (Lagoon, Drift), timelocked risk increases with a
  guardian (Morpho Vault V2), amounts counted by balance change (Kamino).
