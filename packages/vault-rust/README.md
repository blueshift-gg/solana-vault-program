# vault-rust

Rust client for the Solana Vault Program. Everything under `src/generated` is
written by Codama from [`idl/vault.ts`](../../idl/vault.ts); do not edit it.
From the repository root:

```sh
bun install
bun run generate   # idl/vault.json, this crate and packages/vault-kit
cargo test -p vault-tests --test client
```

The generator warns that `solana-account` and `solana-rpc-client` are missing:
they back a `fetch` feature this crate leaves out, see `Cargo.toml`.
