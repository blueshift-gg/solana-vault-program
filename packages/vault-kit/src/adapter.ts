// Resolves the accounts of an adapter call from the adapter's Codama IDL. An
// adapter needs no client of its own: its IDL says where each account is.

import { resolveInstructionAccountAddress } from '@codama/dynamic-address-resolution';
import { getNodeCodec } from '@codama/dynamic-codecs';
import { AccountRole, type AccountMeta, type Address, type ReadonlyUint8Array } from '@solana/kit';
import type { InstructionAccountNode, RootNode } from 'codama';
import type { Vault } from './generated/index.js';

/**
 * The accounts that follow a vault instruction's own when it calls its
 * adapter: the prefix, the adapter program, then the adapter's accounts.
 * `vault` is the decoded vault account, as `fetchVault` returns its data.
 * `Simulate` makes the adapter's `simulate` call, `Allocate` its `deposit`,
 * `Deallocate` and `Fulfil` its `withdraw`.
 *
 * Each adapter account is found from its default in the IDL: a fixed address,
 * a PDA of other accounts, or a field of another account, which `read`
 * fetches. Nothing here is specific to an adapter.
 */
export async function adapterAccounts(
    idl: RootNode,
    call: 'simulate' | 'deposit' | 'withdraw',
    vault: Pick<Vault, 'adapter' | 'assetMint' | 'assetTokenProgram' | 'strategyAccount' | 'strategyAuthority'>,
    read: (address: Address) => Promise<ReadonlyUint8Array>,
): Promise<AccountMeta[]> {
    if (idl.program.publicKey !== vault.adapter) {
        throw new Error(`the IDL is ${idl.program.name}'s, not this vault's adapter's`);
    }
    const ixNode = idl.program.instructions?.find(instruction => instruction.name === call);
    if (!ixNode) throw new Error(`${idl.program.name} has no ${call} instruction`);

    const accounts = ixNode.accounts ?? [];
    // The accounts every adapter call starts with, all named by the vault
    const known: Record<string, Address> = {
        strategyAuthority: vault.strategyAuthority,
        strategyAccount: vault.strategyAccount,
        assetMint: vault.assetMint,
        tokenProgram: vault.assetTokenProgram,
    };
    let pending = accounts.filter(account => !(account.name in known));
    while (pending.length > 0) {
        const waiting: InstructionAccountNode[] = [];
        for (const account of pending) {
            if (!needs(account).every(name => name in known)) {
                waiting.push(account);
                continue;
            }
            const value = account.defaultValue;
            if (value?.kind === 'accountFieldValueNode') {
                const source = accounts.find(other => other.name === value.account);
                const decoded = decode(idl, source, await read(known[value.account]!));
                known[account.name] = decoded[value.path!] as Address;
            } else {
                const address = await resolveInstructionAccountAddress({
                    accountsInput: known,
                    ixAccountNode: account,
                    ixNode,
                    root: idl,
                });
                if (!address) throw new Error(`${call}: ${account.name} has no address`);
                known[account.name] = address;
            }
        }
        if (waiting.length === pending.length) {
            throw new Error(`${call}: cannot resolve ${waiting.map(account => account.name).join(', ')}`);
        }
        pending = waiting;
    }

    const metas = accounts.map(account => ({
        address: known[account.name]!,
        role: account.isWritable ? AccountRole.WRITABLE : AccountRole.READONLY,
    }));
    // The vault signs for the strategy authority itself, and the adapter
    // program sits between the prefix and the adapter's own accounts.
    metas.splice(4, 0, { address: idl.program.publicKey as Address, role: AccountRole.READONLY });
    return metas;
}

/** The accounts whose addresses this one's default is computed from. */
function needs(account: InstructionAccountNode): string[] {
    const value = account.defaultValue;
    if (value?.kind === 'accountFieldValueNode') return [value.account];
    if (value?.kind !== 'pdaValueNode') return [];
    return (value.seeds ?? []).flatMap(seed => (seed.value.kind === 'accountValueNode' ? [seed.value.name] : []));
}

/** Decode an account by the layout its instruction account links to. */
function decode(idl: RootNode, source: InstructionAccountNode | undefined, data: ReadonlyUint8Array) {
    const link = source?.accountLink;
    const program = [idl.program, ...(idl.additionalPrograms ?? [])].find(
        program => program.name === (link?.program?.name ?? idl.program.name),
    );
    const layout = program?.accounts?.find(account => account.name === link?.name);
    if (!program || !layout) throw new Error(`${source?.name} links to no account layout`);
    return getNodeCodec([idl, program, layout]).decode(data) as Record<string, unknown>;
}
