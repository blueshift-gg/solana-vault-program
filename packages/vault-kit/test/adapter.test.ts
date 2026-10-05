// An adapter's accounts, resolved from its IDL alone. The end-to-end tests
// build their adapter accounts from the same IDLs, in the same order and with
// the same flags, and run them against the programs; what is checked here is
// that resolution finds the right addresses.

import { expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { AccountRole, address, getAddressEncoder, getProgramDerivedAddress, type Address } from '@solana/kit';
import type { RootNode } from 'codama';
import { adapterAccounts } from '../src/index.js';

const json = (path: string) => JSON.parse(readFileSync(new URL(`../../../${path}`, import.meta.url), 'utf8'));
const key = (address: Address) => getAddressEncoder().encode(address);
const none = async (): Promise<Uint8Array> => {
    throw new Error('nothing to read');
};

const TOKEN = address('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');
const ASSOCIATED_TOKEN = address('ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL');
const SYSTEM = address('11111111111111111111111111111111');
const STRATEGY_AUTHORITY = address('FW7EWvAXUrj5riXey3uqYEZPM7cHZwYc194gcsXvffYe');
const STRATEGY_ACCOUNT = address('8dndqb1pgEZKNW4ELci4x4WyNZZZ6HJqwrjaAiooN4Ai');

test('custody: the book is a PDA and its token accounts are read from it', async () => {
    const idl: RootNode = json('idl/custody.json');
    const program = address(idl.program.publicKey);
    const vault = {
        adapter: address(idl.program.publicKey),
        strategyAuthority: STRATEGY_AUTHORITY,
        strategyAccount: STRATEGY_ACCOUNT,
        assetMint: SYSTEM,
        assetTokenProgram: TOKEN,
    };
    const [custody] = await getProgramDerivedAddress({
        programAddress: program,
        seeds: ['custody', key(STRATEGY_AUTHORITY)],
    });
    // A custody account as the program lays it out: version, bump, strategy
    // authority, vault, destination, return account, then the rest.
    const [destination, returnAccount] = [ASSOCIATED_TOKEN, TOKEN];
    const data = new Uint8Array(242);
    data.set(key(destination), 66);
    data.set(key(returnAccount), 98);
    const read = async (address: Address) => {
        expect(address).toBe(custody);
        return data;
    };

    const tail = await adapterAccounts(idl, 'withdraw', vault, read);
    expect(tail).toEqual([
        { address: STRATEGY_AUTHORITY, role: AccountRole.WRITABLE },
        { address: STRATEGY_ACCOUNT, role: AccountRole.WRITABLE },
        { address: SYSTEM, role: AccountRole.READONLY },
        { address: TOKEN, role: AccountRole.READONLY },
        { address: program, role: AccountRole.READONLY },
        { address: custody, role: AccountRole.WRITABLE },
        { address: returnAccount, role: AccountRole.WRITABLE },
        { address: destination, role: AccountRole.READONLY },
    ]);
});

test('jupiter lend: every account of the mainnet USDC market resolves from the mint', async () => {
    const idl: RootNode = json('idl/jupiter-lend.json');
    const snapshot = (name: string) => json(`tests/fixtures/${name}.json`);
    const at = (name: string) => address(snapshot(name).pubkey);
    const vault = {
        adapter: address(idl.program.publicKey),
        strategyAuthority: STRATEGY_AUTHORITY,
        strategyAccount: STRATEGY_ACCOUNT,
        assetMint: at('mint'),
        assetTokenProgram: TOKEN,
    };
    const read = async (address: Address) => {
        expect(address).toBe(at('lending'));
        return new Uint8Array(Buffer.from(snapshot('lending').account.data[0], 'base64'));
    };
    const [fTokenAccount] = await getProgramDerivedAddress({
        programAddress: ASSOCIATED_TOKEN,
        seeds: [key(STRATEGY_AUTHORITY), key(TOKEN), key(at('f_token_mint'))],
    });

    const own = async (call: 'simulate' | 'deposit' | 'withdraw', read: (address: Address) => Promise<Uint8Array>) =>
        (await adapterAccounts(idl, call, vault, read)).slice(5).map(meta => meta.address);

    const market = [
        fTokenAccount,
        at('lending'),
        at('lending_admin'),
        at('f_token_mint'),
        at('token_reserve'),
        at('supply_position'),
        at('rate_model'),
        at('liquidity_vault'),
        at('liquidity'),
        address('jupeiUmn818Jg1ekPURTpr4mFo29p46vygyykFJ3wZC'),
        at('rewards_rate_model'),
        ASSOCIATED_TOKEN,
        SYSTEM,
        address('jup3YeL8QhtSx1e253b2FDvsMNC87fDrgQZivbrndc9'),
    ];
    expect(await own('deposit', read)).toEqual(market);
    expect(await own('withdraw', read)).toEqual(market);
    // Reading the position takes no fetch at all.
    expect(await own('simulate', none)).toEqual([fTokenAccount, at('lending'), at('f_token_mint')]);

    // An IDL is only good for the vault whose adapter it describes.
    expect(adapterAccounts(idl, 'simulate', { ...vault, adapter: SYSTEM }, none)).rejects.toThrow();
});
