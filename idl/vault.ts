// The Codama IDL of the vault program, written by hand: the program is
// Pinocchio and has no IDL of its own. `bun run generate` writes idl/vault.json
// and regenerates packages/vault-kit and packages/vault-rust from it.
// tests/tests/client.rs holds this file to the program's wire format.

import { createHash } from 'node:crypto';
import { writeFileSync } from 'node:fs';
import { renderVisitor as renderJs } from '@codama/renderers-js';
import { renderVisitor as renderRust } from '@codama/renderers-rust';
import {
    accountNode,
    argumentValueNode,
    booleanTypeNode,
    bytesTypeNode,
    bytesValueNode,
    constantDiscriminatorNode,
    constantPdaSeedNodeFromString,
    constantValueNode,
    createFromRoot,
    definedTypeLinkNode,
    definedTypeNode,
    enumEmptyVariantTypeNode,
    enumTypeNode,
    errorNode,
    eventNode,
    fieldDiscriminatorNode,
    fixedSizeTypeNode,
    hiddenPrefixTypeNode,
    instructionAccountNode,
    instructionArgumentNode,
    instructionNode,
    instructionRemainingAccountsNode,
    numberTypeNode,
    numberValueNode,
    pdaLinkNode,
    pdaNode,
    programNode,
    publicKeyTypeNode,
    publicKeyValueNode,
    rootNode,
    sizeDiscriminatorNode,
    stringTypeNode,
    optionTypeNode,
    structFieldTypeNode,
    structTypeNode,
    variablePdaSeedNode,
    type InstructionAccountNode,
    type TypeNode,
} from 'codama';

const PROGRAM = '5Bwdadmxt9EbZyKqAN8FspPGDWbxBdWc929LJwo2mMYn';
const EVENT_AUTHORITY = '8dndqb1pgEZKNW4ELci4x4WyNZZZ6HJqwrjaAiooN4Ai';
const SYSTEM_PROGRAM = '11111111111111111111111111111111';
// The default for a share token program; a Token-2022 share mint overrides it.
const TOKEN_PROGRAM = 'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA';

const u8 = numberTypeNode('u8');
const u16 = numberTypeNode('u16');
const u64 = numberTypeNode('u64');
const i64 = numberTypeNode('i64');
const key = publicKeyTypeNode();
const config = definedTypeLinkNode('config');
const status = definedTypeLinkNode('vaultStatus');
const permit = definedTypeLinkNode('permit');
/** Bytes to the end of the instruction data, with no length prefix. */
const rest = bytesTypeNode();

type Fields = Record<string, TypeNode>;
const struct = (fields: Fields) =>
    structTypeNode(Object.entries(fields).map(([name, type]) => structFieldTypeNode({ name, type })));

// ---- Accounts and types ----

const definedTypes = [
    definedTypeNode({
        name: 'config',
        docs: ['Everything about a vault that can change after creation. All-zero keys mean "none".'],
        type: struct({
            manager: key,
            guardian: key,
            feeRecipient: key,
            depositAuthority: key,
            withdrawAuthority: key,
            debtCap: u64,
            depositCap: u64,
            maxAge: u64,
            fulfilDelay: u64,
            unlockPeriod: u64,
            performanceFeeBps: u16,
            managementFeeBps: u16,
        }),
    }),
    definedTypeNode({
        name: 'permit',
        docs: ["An authority's off-chain consent: an ed25519 signature over a `permitMessage`."],
        type: struct({ expiresAt: i64, signature: fixedSizeTypeNode(bytesTypeNode(), 64) }),
    }),
    definedTypeNode({
        name: 'permitKind',
        type: enumTypeNode(['deposit', 'fulfil'].map(name => enumEmptyVariantTypeNode(name))),
    }),
    definedTypeNode({
        name: 'permitMessage',
        docs: [
            'What a permit signs, as permit_message in vault-core builds it. `subject` is the',
            'depositor for a deposit and the ticket for a fulfilment; `nonce` is zero for a',
            "deposit and the ticket's nonce for a fulfilment.",
            'The domain is "solana-vault:v1:". It and the program are plain fields: the Rust',
            'renderer cannot render a type with a constant prefix.',
        ],
        type: struct({
            domain: fixedSizeTypeNode(stringTypeNode('utf8'), 16),
            program: key,
            kind: definedTypeLinkNode('permitKind'),
            vault: key,
            subject: key,
            nonce: u64,
            expiresAt: i64,
        }),
    }),
    definedTypeNode({
        name: 'vaultStatus',
        type: enumTypeNode(['paused', 'active', 'windDown'].map(name => enumEmptyVariantTypeNode(name))),
    }),
];

// Neither account carries a type discriminator; their lengths tell them apart.
const accounts = [
    accountNode({
        name: 'vault',
        size: 837,
        pda: pdaLinkNode('vault'),
        discriminators: [sizeDiscriminatorNode(837)],
        data: struct({
            version: u8,
            status,
            bump: u8,
            strategyBump: u8,
            decimals: u8,
            owner: key,
            pendingOwner: key,
            assetMint: key,
            assetTokenProgram: key,
            shareMint: key,
            idleAccount: key,
            escrowAccount: key,
            adapter: key,
            strategyAuthority: key,
            strategyAccount: key,
            idle: u64,
            debt: u64,
            totalShares: u64,
            feeShares: u64,
            lastReportSlot: u64,
            unlockTs: i64,
            feeTs: i64,
            loss: u64,
            timelock: u64,
            pendingAt: i64,
            locked: u64,
            unlockEnd: i64,
            tickets: u64,
            config,
            pending: config,
        }),
    }),
    accountNode({
        name: 'ticket',
        size: 122,
        pda: pdaLinkNode('ticket'),
        discriminators: [sizeDiscriminatorNode(122)],
        data: struct({
            version: u8,
            bump: u8,
            vault: key,
            owner: key,
            payer: key,
            shares: u64,
            createdAt: i64,
            nonce: u64,
        }),
    }),
];

const seed = (text: string) => constantPdaSeedNodeFromString('utf8', text);
const pdas = [
    pdaNode({ name: 'vault', seeds: [seed('vault'), variablePdaSeedNode('shareMint', key)] }),
    pdaNode({ name: 'strategyAuthority', seeds: [seed('strategy'), variablePdaSeedNode('vault', key)] }),
    pdaNode({
        name: 'ticket',
        seeds: [
            seed('ticket'),
            variablePdaSeedNode('vault', key),
            variablePdaSeedNode('owner', key),
            variablePdaSeedNode('id', u64),
        ],
    }),
    pdaNode({ name: 'eventAuthority', seeds: [seed('__event_authority')] }),
];

// ---- Instructions ----

/** An account; `flags` holds `s` for signer and `w` for writable. */
const account = (name: string, flags = '', defaultAddress?: string) =>
    instructionAccountNode({
        name,
        isSigner: flags.includes('s'),
        isWritable: flags.includes('w'),
        ...(defaultAddress && { defaultValue: publicKeyValueNode(defaultAddress) }),
    });

const discriminatorArgument = (type: TypeNode, defaultValue: Parameters<typeof instructionArgumentNode>[0]['defaultValue']) =>
    instructionArgumentNode({ name: 'discriminator', type, defaultValue, defaultValueStrategy: 'omitted' });

/**
 * The accounts an instruction forwards to the adapter: strategy_authority
 * (may be writable), strategy_account (writable), asset_mint, token_program,
 * the adapter program, then the adapter's own accounts with their own flags.
 */
const adapterAccounts = (isOptional: boolean) =>
    instructionRemainingAccountsNode(argumentValueNode('adapterAccounts'), {
        isOptional,
        docs: [
            'Strategy authority, strategy account (writable), asset mint, asset token program,',
            'adapter program, then the accounts the adapter itself needs.',
        ],
    });

/** A state-changing instruction and the event it emits: `[255, discriminator, fields]`. */
function instruction(
    name: string,
    discriminator: number,
    accounts: InstructionAccountNode[],
    args: Fields,
    event: Fields,
    adapter?: 'required' | 'optional',
) {
    return {
        instruction: instructionNode({
            name,
            accounts: [
                ...accounts,
                account('eventAuthority', '', EVENT_AUTHORITY),
                account('program', '', PROGRAM),
            ],
            arguments: [
                discriminatorArgument(u8, numberValueNode(discriminator)),
                ...Object.entries(args).map(([name, type]) => instructionArgumentNode({ name, type })),
            ],
            ...(adapter && { remainingAccounts: [adapterAccounts(adapter === 'optional')] }),
            discriminators: [fieldDiscriminatorNode('discriminator')],
        }),
        event: (() => {
            const prefix = constantValueNode(
                fixedSizeTypeNode(bytesTypeNode(), 2),
                bytesValueNode('base16', Buffer.from([255, discriminator]).toString('hex')),
            );
            return eventNode({
                name,
                data: hiddenPrefixTypeNode(struct(event), [prefix]),
                discriminators: [constantDiscriminatorNode(prefix)],
            });
        })(),
    };
}

/** An instruction over `RoleAccounts`: the authority and the vault. */
const role = (name: string, discriminator: number, args: Fields, event: Fields) =>
    instruction(name, discriminator, [account('authority', 's'), account('vault', 'w')], args, event);

const managerAccounts = [account('manager', 's'), account('vault', 'w'), account('idleAccount', 'w')];

const instructions = [
    instruction(
        'createVault',
        0,
        [
            account('payer', 'sw'),
            account('owner', 's'),
            account('vault', 'w'),
            account('assetMint'),
            account('shareMint'),
            account('idleAccount'),
            account('escrowAccount'),
            account('strategyAccount'),
            account('systemProgram', '', SYSTEM_PROGRAM),
        ],
        { bump: u8, strategyBump: u8, adapter: key, timelock: u64, config },
        { vault: key, owner: key, assetMint: key, adapter: key },
    ),
    role('submitConfig', 1, { config }, { vault: key, executableAt: i64 }),
    role('executeConfig', 2, {}, { vault: key }),
    role('transferOwnership', 3, { newOwner: key }, { vault: key, newOwner: key }),
    role('acceptOwnership', 4, {}, { vault: key, owner: key }),
    role('windDown', 5, {}, { vault: key }),
    role('setPaused', 10, { paused: booleanTypeNode() }, { vault: key, status }),
    role('writeOff', 11, { value: u64 }, { vault: key, value: u64 }),
    instruction('allocate', 20, managerAccounts, { amount: u64, data: rest }, { vault: key, amount: u64 }, 'required'),
    instruction('deallocate', 21, managerAccounts, { amount: u64, data: rest }, { vault: key, returned: u64 }, 'required'),
    instruction(
        'deposit',
        30,
        [
            account('depositor', 's'),
            account('vault', 'w'),
            account('assetMint'),
            account('shareMint', 'w'),
            account('idleAccount', 'w'),
            account('depositorAssets', 'w'),
            account('depositorShares', 'w'),
            account('assetTokenProgram'),
            account('shareTokenProgram', '', TOKEN_PROGRAM),
        ],
        // `permit` is empty or one encoded `permit`. No flag precedes it, and the
        // Rust renderer has no type for an option without one.
        { assets: u64, minSharesOut: u64, permit: rest },
        { vault: key, depositor: key, assets: u64, shares: u64 },
    ),
    instruction(
        'requestRedeem',
        31,
        [
            account('payer', 'sw'),
            account('owner', 's'),
            account('vault', 'w'),
            account('ticket', 'w'),
            account('shareMint', 'w'),
            account('ownerShares', 'w'),
            account('escrowAccount', 'w'),
            account('idleAccount', 'w'),
            account('assetMint'),
            account('destination', 'w'),
            account('shareTokenProgram', '', TOKEN_PROGRAM),
            account('assetTokenProgram'),
            account('systemProgram', '', SYSTEM_PROGRAM),
        ],
        { shares: u64, id: u64, bump: u8 },
        { vault: key, ticket: key, owner: key, shares: u64, assets: u64 },
    ),
    instruction(
        'cancelRedeem',
        32,
        [
            account('owner', 's'),
            account('payer', 'w'),
            account('vault'),
            account('ticket', 'w'),
            account('shareMint'),
            account('escrowAccount', 'w'),
            account('ownerShares', 'w'),
            account('tokenProgram', '', TOKEN_PROGRAM),
        ],
        {},
        { vault: key, ticket: key, shares: u64 },
    ),
    instruction(
        'simulate',
        40,
        [account('vault', 'w'), account('idleAccount')],
        { data: rest },
        { vault: key, claimed: u64, value: u64, idle: u64 },
        'required',
    ),
    instruction(
        'fulfil',
        41,
        [
            account('vault', 'w'),
            account('ticket', 'w'),
            account('payer', 'w'),
            account('escrowAccount', 'w'),
            account('shareMint', 'w'),
            account('idleAccount', 'w'),
            account('assetMint'),
            account('destination', 'w'),
            account('assetTokenProgram'),
            account('shareTokenProgram', '', TOKEN_PROGRAM),
        ],
        // `has_permit` and the permit are an option with a one-byte flag.
        { permit: optionTypeNode(permit), data: rest },
        { vault: key, ticket: key, assets: u64, shares: u64 },
        'optional',
    ),
    instruction(
        'collectFees',
        42,
        [
            account('vault', 'w'),
            account('shareMint', 'w'),
            account('recipientShares', 'w'),
            account('tokenProgram', '', TOKEN_PROGRAM),
        ],
        {},
        { vault: key, shares: u64 },
    ),
];

/**
 * A view of the read interface: one account, the vault, and the answer as a
 * u64 in return data. The discriminator is the first eight bytes of
 * SHA-256("solana-vault-interface:<name>").
 */
function view(name: string, hashed: string, args: Fields) {
    const discriminator = createHash('sha256').update(`solana-vault-interface:${hashed}`).digest('hex').slice(0, 16);
    return instructionNode({
        name,
        accounts: [account('vault')],
        arguments: [
            discriminatorArgument(fixedSizeTypeNode(bytesTypeNode(), 8), bytesValueNode('base16', discriminator)),
            ...Object.entries(args).map(([name, type]) => instructionArgumentNode({ name, type })),
        ],
        discriminators: [fieldDiscriminatorNode('discriminator')],
    });
}

const views = [
    view('convertToShares', 'convert_to_shares', { assets: u64 }),
    view('convertToAssets', 'convert_to_assets', { shares: u64 }),
    view('maxDeposit', 'max_deposit', {}),
];

// ---- Errors: the code is the position in `VaultError` ----

const errors = [
    ['notMutable', 'An account this program writes directly must be writable'],
    ['notSigner', 'Account expected to be a signer'],
    ['invalidAccountOwner', 'Account expected to be owned by our program'],
    ['invalidAccountLength', 'The account data length is not the expected one'],
    ['invalidVersion', 'The account version is not the expected one'],
    ['alreadyInitialized', 'The account about to be created already exists'],
    ['invalidSeeds', 'The account is not at the address its seeds derive'],
    ['invalidEventAuthority', 'The event CPI was not signed by the event authority'],
    ['invalidAuthority', 'The signer does not hold the role this instruction requires'],
    ['invalidConfig', 'A fee, rate or timelock is above its ceiling'],
    ['timelockNotPassed', 'No configuration is pending, or its timelock has not passed'],
    ['invalidStatus', 'The vault status does not allow this instruction'],
    ['invalidMint', "The mint is not an SPL mint, has a transfer hook, or is not the vault's alone"],
    ['invalidTokenAccount', 'The token account is not the one the vault recorded, or is not clean'],
    ['invalidAdapter', 'The adapter accounts do not match the ones the vault recorded'],
    ['invalidReturnData', "The adapter returned no value, a malformed one, or another program's"],
    ['adapterCallTooLarge', 'An adapter call carries too many accounts or too much data'],
    ['staleReport', "The last report is older than the vault's maximum age"],
    ['zeroAmount', 'The amount is zero or converts to zero'],
    ['amountTooLarge', 'The amount is above idle, a cap, or the value it is bounded by'],
    ['slippageExceeded', 'Fewer shares would be minted than the depositor accepts'],
    ['invalidTicket', 'The ticket does not belong to this vault, owner or payer'],
    ['invalidPermit', "A permit is required and missing, or its signature is not the authority's"],
    ['permitExpired', "The permit's expiry has passed"],
    ['nothingToFulfil', 'Nothing is idle to pay this ticket with'],
    ['mathOverflow', 'A result does not fit in a u64'],
    ['worthless', "The vault's shares are worth nothing right now, so none can be issued"],
].map(([name, message], code) => errorNode({ name: name!, code, message: message! }));

// ---- Output ----

const root = rootNode(
    programNode({
        name: 'solanaVault',
        publicKey: PROGRAM,
        version: '0.0.1',
        accounts,
        definedTypes,
        pdas,
        instructions: [...instructions.map(i => i.instruction), ...views],
        events: instructions.map(i => i.event),
        errors,
    }),
);

const packages = new URL('../packages/', import.meta.url).pathname;
writeFileSync(new URL('vault.json', import.meta.url), JSON.stringify(root, null, 2) + '\n');
const codama = createFromRoot(root);
await codama.accept(renderJs(packages + 'vault-kit', { importExtension: 'js' }));
// Formatted by `cargo fmt` afterwards, with the workspace's own toolchain.
codama.accept(renderRust(packages + 'vault-rust', { anchorTraits: false }));
