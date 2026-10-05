# Solana Vault Program

One deployed program that does share accounting for tokenized vaults. A vault is
an account of this program, not a deployment: one asset in, one share token out,
funds deployed through one adapter program. Strategies, valuation, admission
rules and fee splits are separate programs or keys that plug into it.

Not audited and not deployed; see [Status](#status).

## Trust model

A vault's owner, manager and guardian, and the oracle of a custody vault, are
one trusted party: the operator. Depositors trust the operator with everything
deployed. The program does not try to stop an operator from losing money. It
fixes what the operator cannot do, and makes the rest readable from the vault
account.

What holds for every vault, whatever the operator or the adapter does:

- **The price comes from stored numbers.** Shares are priced from `idle + debt − locked`,
  all stored. A token balance or an adapter's return value is a claim, never
  the price, so a transfer to the vault cannot move it.
- **The price rises slowly.** A gain is locked when it is counted and reaches
  the share price evenly over the vault's unlock period; a loss reaches it at
  once. A deposit is priced with locked gains included and a redemption without
  them, so capital that arrives for a gain and leaves right after gets none.
- **The adapter is contained.** Its only signer owns only the strategy token
  account. Idle funds and the share mint are never within its reach.
- **Configuration changes wait.** Roles, fees, caps and limits change only
  after the vault's timelock, which is fixed at creation.
- **Status never blocks an exit.** Pause and wind-down stop deposits and
  allocations only. Requesting, cancelling and fulfilling keep working.
- **A withdrawal authority's priority ends.** After the fulfil delay (at most
  90 days) anyone can fulfil any ticket.
- **Rounding favours the vault,** and one virtual share makes the
  first-depositor inflation attack worthless.

What the operator can do:

- Lose deployed funds, up to the debt cap. Losses reach the price at once.
- Write the strategy's value down to any amount, instantly (guardian).
- Choose the adapter at creation. The vault fixes its program id, not its code:
  an upgradeable adapter can change.
- Stall exits by not reporting. `Deposit` and `Fulfil` need a report within
  `max_age` whenever the vault has debt, so an adapter that stops answering, or
  a custody oracle that stops signing, blocks both until the guardian writes
  the debt off.
- Set a timelock of zero or an unbounded `max_age`. Neither has a floor; read
  them from the vault account.

## How it works

```text
deposit ──► idle ──allocate──► strategy account ──► adapter ──► protocol
              ▲                                        │
              └────────── deallocate / fulfil ◄────────┘

simulate: adapter's claim + idle balance ──► stored idle and debt; gains locked
```

Exits go through a ticket. `RequestRedeem` escrows shares, and `Fulfil` pays
them at the price at that moment, from idle first and then by pulling the
shortfall from the adapter. A ticket that idle cannot cover is paid in part and
stays open. `CancelRedeem` returns the shares.

Two recipes cover most clients, each in one transaction:

| To | Send |
|---|---|
| Deposit | `Simulate`, `Deposit` |
| Withdraw now | `Simulate`, `RequestRedeem`, `Fulfil` |

`Simulate` is needed whenever the vault has debt and its last report is older
than `max_age`. It takes the adapter's accounts; the others do not, except a
`Fulfil` that pulls from the strategy.

## Instructions

| Role | Instruction | Discriminator |
|---|---|---|
| Owner | `CreateVault`, `SubmitConfig`, `ExecuteConfig`, `TransferOwnership`, `AcceptOwnership`, `WindDown` | 0 to 5 |
| Guardian | `SetPaused`, `WriteOff` | 10, 11 |
| Manager | `Allocate`, `Deallocate` | 20, 21 |
| Holder | `Deposit`, `RequestRedeem`, `CancelRedeem` | 30 to 32 |
| Anyone | `Simulate`, `Fulfil`, `CollectFees` | 40 to 42 |

Accounts, parameters and checks are documented on each instruction's struct
under [program/src](program/src). Every state-changing instruction emits one
event by self-CPI: byte 255, the instruction's discriminator, then the fields.

A vault's configuration, held in the vault account and changed through the
timelock: manager, guardian, fee recipient, optional deposit authority,
optional withdrawal authority, debt cap, deposit cap, maximum report age in
slots, fulfil delay, unlock period, performance fee (at most 50% of gains) and
management fee (at most 5% a year). Fees are paid in shares.

Addresses: the vault is the PDA `["vault", share_mint]`, the strategy authority
`["strategy", vault]`, a ticket `["ticket", vault, owner, id]` with a
client-chosen `id`.

## What plugs in

| Plug | How |
|---|---|
| Strategy and valuation | An adapter program, below |
| Reading a vault from another program | The read interface, below |
| Who may deposit; who is paid first | A key that signs permits off chain |
| Allocation policy | The manager is any signer, including another program's PDA |
| Fee split | The fee recipient is any address |

### Adapter interface

An adapter is a program with three instructions. Each discriminator is the
first eight bytes of `SHA-256("solana-vault-adapter:<name>")`.

| Instruction | Does |
|---|---|
| `simulate(data)` | Sets return data to the position's value as a `u64` LE, not counting the strategy account |
| `deposit(amount, data)` | Takes `amount` out of the strategy account |
| `withdraw(amount, data)` | Puts up to `amount` back into it |

Every call starts with the same four accounts: strategy authority (signer on
`deposit` and `withdraw`), strategy token account, asset mint, token program.
The adapter's own accounts follow, forwarded with their writable flag and never
as signers. A call carries at most 32 accounts and 256 bytes of `data`.

`Simulate` and `Fulfil` are permissionless, so `data` and the adapter's own
accounts can come from anyone. An adapter must validate them itself. The vault
counts principal by the balance change in the strategy account, not by anything
the adapter returns.

The constants live in [constants.rs](packages/vault-core/src/constants.rs).

#### Finding an adapter's accounts

Nothing on chain lists the accounts an adapter takes: the vault forwards what
it is given and the adapter checks it. The list is the adapter's
[Codama](https://github.com/codama-idl/codama) IDL. It declares the three
instructions with the prefix first, and gives each account after it a default
of one of three kinds:

| The account is | Codama node |
|---|---|
| A fixed address | `publicKeyValueNode` |
| A PDA of other accounts and constants | `pdaValueNode` |
| A key stored in another account | `accountFieldValueNode` |

No custom resolver: an account that fits none of these is a reason to change
the adapter. `adapterAccounts` in [vault-kit](packages/vault-kit/src/adapter.ts)
resolves any conforming adapter's accounts from its IDL and the vault account,
with no code per adapter:

```ts
const { data: vault } = await fetchVault(rpc, address);
const read = async (address: Address) => {
    const account = await fetchEncodedAccount(rpc, address);
    assertAccountExists(account);
    return account.data;
};
const tail = await adapterAccounts(idl, 'simulate', vault, read);
```

The IDLs of the two adapters here are written in
[idl/adapters.ts](idl/adapters.ts); the end-to-end tests take their adapter
accounts from them. An adapter's author publishes its IDL however they like.
To make it findable from the vault account alone, store it with the
[Program Metadata](https://github.com/solana-program/program-metadata) program
under the seed `idl`, where its client's `fetchAndParseMetadataContent(rpc,
vault.adapter, 'idl')` reads it back. No program reads it.

### Read interface

Three views take the vault account alone and answer with a `u64` LE in return
data, by CPI or by simulation. Each discriminator is the first eight bytes of
`SHA-256("solana-vault-interface:<name>")`.

| View | Answers |
|---|---|
| `convert_to_shares(assets)` | Shares a deposit would mint |
| `convert_to_assets(shares)` | Assets a redemption would pay |
| `max_deposit()` | Largest deposit accepted now |

Views answer from stored values and do not check freshness. Run `Simulate`
first for a current price.

### Permits

A deposit or withdrawal authority never co-signs a transaction. It signs an
Ed25519 message off chain binding the program, the permit kind, the vault, the
subject (the depositor, or the ticket) and an expiry. Whoever holds the permit
presents it. It is reusable until it expires. The message is `permit_message`
in [vault-core](packages/vault-core/src/lib.rs).

## Adapters in this repository

| Adapter | Funds are | Value comes from |
|---|---|---|
| [Jupiter Lend](adapters/jupiter-lend) | In the lending market, as fTokens | The exchange price the market last stored |
| [Custody](adapters/custody) | Off chain, sent to one token account fixed at setup | An oracle's signed price per unit |

**Jupiter Lend** is stateless. The vault's strategy account must be the
strategy authority's associated token account. Under Mollusk, against a
snapshot of the mainnet USDC market and the real lending programs: allocate
70,657 compute units, simulate 8,517, a fulfilment that pulls its shortfall
from the market 58,889.

**Custody** keeps a book in units. Funds going out buy units at the current
price and funds coming back redeem them. The custodian returns funds by a plain
transfer to an account the adapter owns. Nothing can make a custodian send
funds back, and a price past its expiry stops the vault pricing anything until
the oracle signs again.

## Limits

- One adapter per vault, fixed at creation. A multi-protocol strategy is a
  composite adapter someone has to write.
- The asset and the share mint may each be SPL Token or Token-2022. The one thing
  neither may have is an active transfer hook; everything else about a mint is
  for whoever chose it to weigh.
- The adapter's accounts are not stored on chain. A client resolves them from
  the adapter's IDL, and only in TypeScript: the Rust client takes them as
  remaining accounts.
- Tickets are unordered, and redemption has no slippage bound.
- A fulfilment that pulls from Jupiter Lend is five programs deep with the
  vault at the top, so another program cannot make that call by CPI.
- After a write-off, funds the strategy returns beyond the remaining debt stay
  in the strategy account and reach the price through `Simulate` as a gain,
  over the unlock period.

## Layout

| Path | Holds |
|---|---|
| [program](program) | The Pinocchio program: `no_std`, no heap, one struct per instruction |
| [packages/vault-core](packages/vault-core) | Account layouts, constants, errors and the [math module](packages/vault-core/src/math/formula.rs) |
| [adapters](adapters) | Jupiter Lend and Custody |
| [packages/vault-kit](packages/vault-kit), [packages/vault-rust](packages/vault-rust) | TypeScript and Rust clients, generated by Codama from [idl/vault.ts](idl/vault.ts) |
| [idl](idl) | The vault's IDL and the adapters', each written by hand |
| [tests](tests) | End-to-end tests under Mollusk: the vault behind the custody adapter, and on Jupiter Lend itself |
| [verification](verification) | Lean proofs about the math module, as translated from the Rust by Charon and Aeneas |

## Build and test

```sh
cargo build-sbf --manifest-path program/Cargo.toml
cargo build-sbf --manifest-path adapters/jupiter-lend/Cargo.toml
cargo build-sbf --manifest-path adapters/custody/Cargo.toml
tests/fetch-fixtures.sh   # once: dumps Jupiter Lend's programs from mainnet
cargo test --workspace
```

## Status

Version 0.0.1. The program id in `vault-core`, and the event authority derived
from it, come from a local keypair and must be replaced before deploying.
