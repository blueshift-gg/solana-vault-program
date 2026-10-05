// The expected values come from the Rust client (`Vault::find_pda`,
// `Ticket::find_pda`, `DepositBuilder`) and `vault_core::permit_message`, which
// tests/tests/client.rs holds to the program.

import { expect, test } from 'bun:test';
import { AccountRole, address, createNoopSigner } from '@solana/kit';
import {
    findEventAuthorityPda,
    findTicketPda,
    findVaultPda,
    getDepositInstruction,
    getPermitMessageEncoder,
    PermitKind,
    getSimulateInstruction,
    SOLANA_VAULT_PROGRAM_ADDRESS,
} from '../src/index.js';

const TOKEN = address('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');
const TOKEN_2022 = address('TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb');
const VAULT = address('FW7EWvAXUrj5riXey3uqYEZPM7cHZwYc194gcsXvffYe');
const EVENT_AUTHORITY = address('8dndqb1pgEZKNW4ELci4x4WyNZZZ6HJqwrjaAiooN4Ai');

test('PDAs derive as in Rust', async () => {
    expect<unknown>(await findVaultPda({ shareMint: TOKEN })).toEqual([VAULT, 255]);
    expect<unknown>(await findTicketPda({ vault: VAULT, owner: TOKEN, id: 7 })).toEqual([
        address('BiGTbKdYoubgYZwDhuiymxKq87EvciuvTCmZToTp7VZE'),
        255,
    ]);
    expect<unknown>(await findEventAuthorityPda()).toEqual([EVENT_AUTHORITY, 254]);
});

test('deposit encodes as in Rust', () => {
    const ix = getDepositInstruction({
        depositor: createNoopSigner(TOKEN),
        vault: VAULT,
        assetMint: TOKEN,
        shareMint: TOKEN,
        idleAccount: TOKEN,
        depositorAssets: TOKEN,
        depositorShares: TOKEN,
        assetTokenProgram: TOKEN_2022,
        assets: 1_000,
        minSharesOut: 999,
        permit: new Uint8Array(),
    });
    expect([...ix.data]).toEqual([30, 232, 3, 0, 0, 0, 0, 0, 0, 231, 3, 0, 0, 0, 0, 0, 0]);
    expect(ix.programAddress).toBe(SOLANA_VAULT_PROGRAM_ADDRESS);
    // The defaults: SPL Token for shares, then the event authority and the program.
    expect(ix.accounts.slice(8).map(a => a.address)).toEqual([TOKEN, EVENT_AUTHORITY, SOLANA_VAULT_PROGRAM_ADDRESS]);
    expect(ix.accounts.map(a => a.role)).toEqual([
        AccountRole.READONLY_SIGNER,
        AccountRole.WRITABLE,
        AccountRole.READONLY,
        AccountRole.WRITABLE,
        AccountRole.WRITABLE,
        AccountRole.WRITABLE,
        AccountRole.WRITABLE,
        AccountRole.READONLY,
        AccountRole.READONLY,
        AccountRole.READONLY,
        AccountRole.READONLY,
    ]);
});

test('adapter accounts keep their own writable flags', () => {
    const ix = getSimulateInstruction({
        vault: VAULT,
        idleAccount: TOKEN,
        data: new Uint8Array(),
        adapterAccounts: [TOKEN, { address: TOKEN_2022, role: AccountRole.WRITABLE }],
    });
    expect([...ix.data]).toEqual([40]);
    expect(ix.accounts.slice(4)).toEqual([
        { address: TOKEN, role: AccountRole.READONLY },
        { address: TOKEN_2022, role: AccountRole.WRITABLE },
    ]);
});

test('the permit message encodes as in Rust', () => {
    const message = getPermitMessageEncoder().encode({
        domain: 'solana-vault:v1:',
        program: SOLANA_VAULT_PROGRAM_ADDRESS,
        kind: PermitKind.Fulfil,
        vault: VAULT,
        subject: TOKEN,
        nonce: 9,
        expiresAt: 1234,
    });
    expect(Buffer.from(message).toString('hex')).toBe(
        '736f6c616e612d7661756c743a76313a3e3c53b07f4cf49409705fd2b18f654329f35fdce9c79067798f7f82f3adb7b3' +
            '01d7788215a6414613911dc5b61309d48f3da43e32ed7b47ae8e9540e1b034268306ddf6e1d765a193d9cbe146ceeb79' +
            'ac1cb485ed5f5b37913a8cf5857eff00a90900000000000000d204000000000000',
    );
});
