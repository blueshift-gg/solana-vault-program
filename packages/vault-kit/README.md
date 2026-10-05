# vault-kit

`@solana/kit` client for the Solana Vault Program. Everything under
`src/generated` is written by Codama from [`idl/vault.ts`](../../idl/vault.ts);
do not edit it. [`src/adapter.ts`](src/adapter.ts) is written by hand: it resolves
an adapter call's accounts from the adapter's own IDL. From the repository root:

```sh
bun install
bun run generate   # idl/vault.json, this package and packages/vault-rust
bun run test
```
