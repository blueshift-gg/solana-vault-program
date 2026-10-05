// The Codama IDLs of the adapters in this repository. An adapter's IDL is its
// account list: each account the adapter takes after the interface prefix
// carries a default that says where it is, as a fixed address, a PDA, or a
// field of another account. `adapterAccounts` in vault-kit resolves a call's
// accounts from that alone, and the end-to-end tests take theirs from the same
// files, so the programs hold the lists to what they accept.
//
// `bun run generate` writes idl/custody.json and idl/jupiter-lend.json.

import { createHash } from 'node:crypto';
import { writeFileSync } from 'node:fs';
import {
    accountFieldValueNode,
    accountLinkNode,
    accountNode,
    accountValueNode,
    bytesTypeNode,
    bytesValueNode,
    constantPdaSeedNodeFromString,
    fieldDiscriminatorNode,
    fixedSizeTypeNode,
    instructionAccountNode,
    instructionArgumentNode,
    instructionNode,
    numberTypeNode,
    pdaNode,
    pdaSeedValueNode,
    pdaValueNode,
    programNode,
    publicKeyTypeNode,
    publicKeyValueNode,
    rootNode,
    structFieldTypeNode,
    structTypeNode,
    variablePdaSeedNode,
    type InstructionAccountNode,
    type InstructionInputValueNode,
    type TypeNode,
} from 'codama';

const SYSTEM_PROGRAM = '11111111111111111111111111111111';
const ASSOCIATED_TOKEN_PROGRAM = 'ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL';

const u8 = numberTypeNode('u8');
const u16 = numberTypeNode('u16');
const u64 = numberTypeNode('u64');
const i64 = numberTypeNode('i64');
const key = publicKeyTypeNode();
/** Bytes to the end of the instruction data, with no length prefix. */
const rest = bytesTypeNode();

type Fields = Record<string, TypeNode>;
const struct = (fields: Fields) =>
    structTypeNode(Object.entries(fields).map(([name, type]) => structFieldTypeNode({ name, type })));

/** `flags` holds `s` for a signer and `w` for writable. */
const account = (
    name: string,
    flags = '',
    defaultValue?: InstructionInputValueNode,
    accountLink?: ReturnType<typeof accountLinkNode>,
): InstructionAccountNode =>
    instructionAccountNode({
        name,
        isSigner: flags.includes('s'),
        isWritable: flags.includes('w'),
        ...(defaultValue && { defaultValue }),
        ...(accountLink && { accountLink }),
    });

const fixed = (address: string) => publicKeyValueNode(address);
const field = (account: string, path: string) => accountFieldValueNode({ account, path });

/**
 * A PDA of `program`. A string seed is a constant; `{ name }` is the address
 * of the account of that name in the same instruction.
 */
function pda(name: string, program: string, seeds: (string | { account: string })[]) {
    const variable = seeds.filter(seed => typeof seed !== 'string');
    return pdaValueNode(
        pdaNode({
            name,
            programId: program,
            seeds: seeds.map(seed =>
                typeof seed === 'string'
                    ? constantPdaSeedNodeFromString('utf8', seed)
                    : variablePdaSeedNode(seed.account, key),
            ),
        }),
        variable.map(seed => pdaSeedValueNode(seed.account, accountValueNode(seed.account))),
    );
}

/** The four accounts every adapter call starts with, which the vault supplies. */
const prefix = (movesFunds: boolean) => [
    account('strategyAuthority', movesFunds ? 'sw' : ''),
    account('strategyAccount', 'w'),
    account('assetMint'),
    account('tokenProgram'),
];

/** An instruction whose discriminator is the first eight bytes of SHA-256(`hashed`). */
function instruction(name: string, hashed: string, accounts: InstructionAccountNode[], args: Fields) {
    const discriminator = createHash('sha256').update(hashed).digest('hex').slice(0, 16);
    return instructionNode({
        name,
        accounts,
        arguments: [
            instructionArgumentNode({
                name: 'discriminator',
                type: fixedSizeTypeNode(bytesTypeNode(), 8),
                defaultValue: bytesValueNode('base16', discriminator),
                defaultValueStrategy: 'omitted',
            }),
            ...Object.entries(args).map(([name, type]) => instructionArgumentNode({ name, type })),
        ],
        discriminators: [fieldDiscriminatorNode('discriminator')],
    });
}

/** An instruction of the adapter interface: the prefix, then the adapter's own accounts. */
const call = (name: 'simulate' | 'deposit' | 'withdraw', own: InstructionAccountNode[], args: Fields) =>
    instruction(name, `solana-vault-adapter:${name}`, [...prefix(name !== 'simulate'), ...own], args);

// ---- Custody ----

const CUSTODY = 'PRBu3zLbyGKYfzkmA7dsVfLcfLwtz9bgGBoVnSgGHK9';

/** `w` names the accounts this call writes to, besides the custody itself. */
const book = (w: string) => [
    account('custody', 'w', pda('custody', CUSTODY, ['custody', { account: 'strategyAuthority' }]), accountLinkNode('custody')),
    account('returnAccount', w.includes('r') ? 'w' : '', field('custody', 'returnAccount')),
    account('destination', w.includes('d') ? 'w' : '', field('custody', 'destination')),
];

const custody = rootNode(
    programNode({
        name: 'custodyAdapter',
        publicKey: CUSTODY,
        version: '0.0.1',
        accounts: [
            accountNode({
                name: 'custody',
                docs: ["One vault's off-chain book."],
                data: struct({
                    version: u8,
                    bump: u8,
                    strategyAuthority: key,
                    vault: key,
                    destination: key,
                    returnAccount: key,
                    oracle: key,
                    pendingOracle: key,
                    pendingOracleAt: i64,
                    units: u64,
                    price: u64,
                    priceExpiresAt: i64,
                    settled: u64,
                    book: u64,
                }),
            }),
        ],
        instructions: [
            // `report` is empty or a signed price: price, expiry, signature.
            call('simulate', book(''), { report: rest }),
            call('deposit', book('d'), { amount: u64 }),
            call('withdraw', book('r'), { amount: u64 }),
            // The custody is the PDA of the vault's strategy authority, which
            // is a field of the vault and not an account here.
            instruction(
                'initialize',
                'solana-vault-custody:initialize',
                [
                    account('payer', 'sw'),
                    account('owner', 's'),
                    account('vault'),
                    account('custody', 'w'),
                    account('destination'),
                    account('returnAccount'),
                    account('systemProgram', '', fixed(SYSTEM_PROGRAM)),
                ],
                { oracle: key },
            ),
            instruction(
                'setOracle',
                'solana-vault-custody:set_oracle',
                [account('owner', 's'), account('vault'), account('custody', 'w')],
                { oracle: key },
            ),
        ],
    }),
);

// ---- Jupiter Lend ----

const JUPITER_LEND = '7T9qpK5R9oFtrXjRfS7r17cozooRDyemkCLPziBUJL18';
// The seeds are the lending and liquidity programs' own
// (github.com/Instadapp/fluid-solana-programs); the tests resolve them
// against a snapshot of the mainnet USDC market.
const LENDING = 'jup3YeL8QhtSx1e253b2FDvsMNC87fDrgQZivbrndc9';
const LIQUIDITY = 'jupeiUmn818Jg1ekPURTpr4mFo29p46vygyykFJ3wZC';

const assetMint = { account: 'assetMint' };
const fTokenMint = (flags = '') =>
    account('fTokenMint', flags, pda('fTokenMint', LENDING, ['f_token_mint', assetMint]));
const position = (flags = '') => [
    account(
        'fTokenAccount',
        flags,
        pda('fTokenAccount', ASSOCIATED_TOKEN_PROGRAM, [
            { account: 'strategyAuthority' },
            { account: 'tokenProgram' },
            { account: 'fTokenMint' },
        ]),
    ),
    account(
        'lending',
        flags,
        pda('lending', LENDING, ['lending', assetMint, { account: 'fTokenMint' }]),
        accountLinkNode('lending', 'jupiterLending'),
    ),
];
const market = [
    ...position('w'),
    account('lendingAdmin', '', pda('lendingAdmin', LENDING, ['lending_admin'])),
    fTokenMint('w'),
    account('tokenReserve', 'w', pda('tokenReserve', LIQUIDITY, ['reserve', assetMint])),
    account(
        'supplyPosition',
        'w',
        pda('supplyPosition', LIQUIDITY, ['user_supply_position', assetMint, { account: 'lending' }]),
    ),
    account('rateModel', '', pda('rateModel', LIQUIDITY, ['rate_model', assetMint])),
    account(
        'liquidityVault',
        'w',
        pda('liquidityVault', ASSOCIATED_TOKEN_PROGRAM, [
            { account: 'liquidity' },
            { account: 'tokenProgram' },
            assetMint,
        ]),
    ),
    account('liquidity', 'w', pda('liquidity', LIQUIDITY, ['liquidity'])),
    account('liquidityProgram', '', fixed(LIQUIDITY)),
    account('rewardsRateModel', '', field('lending', 'rewardsRateModel')),
    account('associatedTokenProgram', '', fixed(ASSOCIATED_TOKEN_PROGRAM)),
    account('systemProgram', '', fixed(SYSTEM_PROGRAM)),
    account('lendingProgram', '', fixed(LENDING)),
];

const jupiterLend = rootNode(
    programNode({
        name: 'jupiterLendAdapter',
        publicKey: JUPITER_LEND,
        version: '0.0.1',
        instructions: [
            // The adapter reads the position only. The fToken mint is last and
            // unread: it is here because the other two derive from it.
            call('simulate', [...position(), fTokenMint()], {}),
            call('deposit', market, { amount: u64 }),
            call('withdraw', market, { amount: u64 }),
        ],
    }),
    [
        programNode({
            name: 'jupiterLending',
            publicKey: LENDING,
            version: '0.0.0',
            accounts: [
                accountNode({
                    name: 'lending',
                    docs: ['The start of a market account: as far as the fields an adapter call needs.'],
                    data: struct({
                        discriminator: fixedSizeTypeNode(bytesTypeNode(), 8),
                        mint: key,
                        fTokenMint: key,
                        lendingId: u16,
                        decimals: u8,
                        rewardsRateModel: key,
                    }),
                }),
            ],
        }),
    ],
);

writeFileSync(new URL('./custody.json', import.meta.url), JSON.stringify(custody, null, 2) + '\n');
writeFileSync(new URL('./jupiter-lend.json', import.meta.url), JSON.stringify(jupiterLend, null, 2) + '\n');
