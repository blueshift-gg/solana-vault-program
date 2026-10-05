//! End-to-end test fixture: a Mollusk instance running the vault program with
//! the token programs and a real adapter behind it, one builder per
//! instruction, and readers for the state a test asserts on. Accounts are
//! packed by hand, exactly as they sit on chain.
//!
//! `custody` and `jupiter` set a vault up behind each shipped adapter.

use mollusk_svm::{program::keyed_account_for_system_program, result::InstructionResult, Mollusk};
use solana_account::Account;
use solana_instruction::{AccountMeta, Instruction};
use solana_pubkey::Pubkey;
use vault_core::constants::*;
use vault_core::errors::VaultError;
use vault_core::state::{Ticket, Vault};

pub use ed25519_dalek::SigningKey;

pub mod custody;
pub mod jupiter;

pub const PROGRAM_ID: Pubkey = Pubkey::new_from_array(vault_core::ID);
pub const TOKEN: Pubkey = mollusk_svm_programs_token::token::ID;
pub const TOKEN_2022: Pubkey = mollusk_svm_programs_token::token2022::ID;
pub const SYSTEM: Pubkey = Pubkey::new_from_array([0; 32]);
pub const EVENT_AUTHORITY: Pubkey = Pubkey::new_from_array(vault_core::constants::EVENT_AUTHORITY);

pub const DECIMALS: u8 = 6;
pub const USER_BALANCE: u64 = 1_000_000;
pub const YEAR: i64 = 365 * 24 * 60 * 60;

pub fn wallet(lamports: u64) -> Account {
    Account {
        lamports,
        data: vec![],
        owner: SYSTEM,
        executable: false,
        rent_epoch: 0,
    }
}

/// A token account under `program` (165 bytes), initialized, with no delegate
/// and no close authority.
pub fn token_account(program: &Pubkey, mint: &Pubkey, owner: &Pubkey, amount: u64) -> Account {
    let mut data = vec![0u8; 165];
    data[..32].copy_from_slice(mint.as_ref());
    data[32..64].copy_from_slice(owner.as_ref());
    data[64..72].copy_from_slice(&amount.to_le_bytes());
    data[108] = 1;
    Account {
        lamports: 2_039_280,
        data,
        owner: *program,
        executable: false,
        rent_epoch: 0,
    }
}

/// A mint under `program` with `DECIMALS` and an optional mint authority.
/// `extensions` are Token-2022 `(type, value)` entries, appended as TLV.
pub fn mint_account(
    program: &Pubkey,
    authority: Option<&Pubkey>,
    extensions: &[(u16, Vec<u8>)],
) -> Account {
    let mut data = vec![0u8; 82];
    if let Some(authority) = authority {
        data[0] = 1;
        data[4..36].copy_from_slice(authority.as_ref());
    }
    data[44] = DECIMALS;
    data[45] = 1;
    if !extensions.is_empty() {
        data.resize(165, 0);
        data.push(1); // account type: mint
        for (extension, value) in extensions {
            data.extend(extension.to_le_bytes());
            data.extend((value.len() as u16).to_le_bytes());
            data.extend(value);
        }
    }
    Account {
        lamports: 10_000_000,
        data,
        owner: *program,
        executable: false,
        rent_epoch: 0,
    }
}

pub fn token_amount(account: &Account) -> u64 {
    u64::from_le_bytes(account.data[64..72].try_into().unwrap())
}

pub fn custom_error(result: &InstructionResult) -> Option<u32> {
    match &result.program_result {
        mollusk_svm::result::ProgramResult::Failure(
            solana_program_error::ProgramError::Custom(c),
        ) => Some(*c),
        _ => None,
    }
}

/// A vault configuration as a test writes it; `bytes` is the wire layout.
#[derive(Clone, Copy)]
pub struct ConfigArgs {
    pub manager: Pubkey,
    pub guardian: Pubkey,
    pub fee_recipient: Pubkey,
    pub deposit_authority: Pubkey,
    pub withdraw_authority: Pubkey,
    pub debt_cap: u64,
    pub deposit_cap: u64,
    pub max_age: u64,
    pub fulfil_delay: u64,
    pub unlock_period: u64,
    pub performance_fee_bps: u16,
    pub management_fee_bps: u16,
}

impl ConfigArgs {
    pub fn bytes(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(CONFIG_LEN);
        data.extend(self.manager.as_ref());
        data.extend(self.guardian.as_ref());
        data.extend(self.fee_recipient.as_ref());
        data.extend(self.deposit_authority.as_ref());
        data.extend(self.withdraw_authority.as_ref());
        data.extend(self.debt_cap.to_le_bytes());
        data.extend(self.deposit_cap.to_le_bytes());
        data.extend(self.max_age.to_le_bytes());
        data.extend(self.fulfil_delay.to_le_bytes());
        data.extend(self.unlock_period.to_le_bytes());
        data.extend(self.performance_fee_bps.to_le_bytes());
        data.extend(self.management_fee_bps.to_le_bytes());
        data
    }
}

pub struct Fixture {
    pub mollusk: Mollusk,
    pub accounts: Vec<(Pubkey, Account)>,
    /// The asset's token program: SPL Token or Token-2022.
    pub token_program: Pubkey,
    /// The share mint's token program.
    pub share_program: Pubkey,
    /// The vault's adapter program.
    pub adapter: Pubkey,
    pub config: ConfigArgs,
    pub timelock: u64,

    pub payer: Pubkey,
    pub owner: Pubkey,
    pub manager: Pubkey,
    pub guardian: Pubkey,
    pub fee_recipient: Pubkey,
    pub user: Pubkey,
    pub stranger: Pubkey,

    pub asset_mint: Pubkey,
    pub share_mint: Pubkey,
    pub vault: Pubkey,
    pub idle_account: Pubkey,
    pub escrow_account: Pubkey,
    pub strategy_authority: Pubkey,
    pub strategy_account: Pubkey,
    /// The adapter's own accounts, after the interface prefix and the program.
    pub adapter_own: Vec<AccountMeta>,

    pub user_assets: Pubkey,
    pub user_shares: Pubkey,
    pub fee_shares: Pubkey,
}

/// The key a test signs permits with, as a deposit or withdrawal authority would.
pub fn authority() -> SigningKey {
    SigningKey::from_bytes(&[7; 32])
}

pub fn authority_key() -> Pubkey {
    Pubkey::new_from_array(authority().verifying_key().to_bytes())
}

impl Fixture {
    /// Every account a vault needs, with the vault itself not created yet, so
    /// a test can tamper with the accounts or the configuration first.
    pub fn setup(adapter: Pubkey, token_program: Pubkey) -> Self {
        Self::setup_with_shares(adapter, token_program, TOKEN)
    }

    /// `setup`, with the share mint under `share_program`.
    pub fn setup_with_shares(
        adapter: Pubkey,
        token_program: Pubkey,
        share_program: Pubkey,
    ) -> Self {
        std::env::set_var(
            "SBF_OUT_DIR",
            concat!(env!("CARGO_MANIFEST_DIR"), "/../target/deploy"),
        );
        let mut mollusk = Mollusk::new(&PROGRAM_ID, "vault_program");
        mollusk_svm_programs_token::token::add_program(&mut mollusk);
        mollusk_svm_programs_token::token2022::add_program(&mut mollusk);
        mollusk.sysvars.clock.unix_timestamp = 1_000_000;
        // Mainnet's limit: an instruction and four nested CPIs. Mollusk turns
        // the raised limit on by default, which would hide a path too deep.
        mollusk.feature_set.raise_cpi_nesting_limit_to_8 = false;

        let [payer, owner, manager, guardian, fee_recipient, user, stranger] =
            core::array::from_fn(|_| Pubkey::new_unique());
        let [asset_mint, share_mint, idle_account, escrow_account, strategy_account] =
            core::array::from_fn(|_| Pubkey::new_unique());
        let [user_assets, user_shares, fee_shares] = core::array::from_fn(|_| Pubkey::new_unique());
        let vault = Pubkey::find_program_address(&[VAULT_SEED, share_mint.as_ref()], &PROGRAM_ID).0;
        let strategy_authority =
            Pubkey::find_program_address(&[STRATEGY_SEED, vault.as_ref()], &PROGRAM_ID).0;

        let asset =
            |owner: &Pubkey, amount| token_account(&token_program, &asset_mint, owner, amount);
        let share = |owner: &Pubkey| token_account(&share_program, &share_mint, owner, 0);
        let accounts = vec![
            (payer, wallet(10_000_000_000)),
            (owner, wallet(10_000_000_000)),
            (manager, wallet(10_000_000_000)),
            (guardian, wallet(10_000_000_000)),
            (user, wallet(10_000_000_000)),
            (stranger, wallet(10_000_000_000)),
            (asset_mint, mint_account(&token_program, None, &[])),
            (share_mint, mint_account(&share_program, Some(&vault), &[])),
            (idle_account, asset(&vault, 0)),
            (escrow_account, share(&vault)),
            (strategy_account, asset(&strategy_authority, 0)),
            (user_assets, asset(&user, USER_BALANCE)),
            (user_shares, share(&user)),
            (fee_shares, share(&fee_recipient)),
            (vault, wallet(0)),
            keyed_account_for_system_program(),
            // The program's own account: every instruction passes it for the event CPI.
            (
                PROGRAM_ID,
                mollusk_svm::program::create_program_account_loader_v3(&PROGRAM_ID),
            ),
            mollusk_svm_programs_token::token::keyed_account(),
            mollusk_svm_programs_token::token2022::keyed_account(),
        ];

        Self {
            mollusk,
            accounts,
            token_program,
            share_program,
            adapter,
            config: ConfigArgs {
                manager,
                guardian,
                fee_recipient,
                deposit_authority: SYSTEM,
                withdraw_authority: SYSTEM,
                debt_cap: u64::MAX,
                deposit_cap: u64::MAX,
                max_age: 0,
                fulfil_delay: 0,
                unlock_period: 0,
                performance_fee_bps: 0,
                management_fee_bps: 0,
            },
            timelock: 1_000,
            payer,
            owner,
            manager,
            guardian,
            fee_recipient,
            user,
            stranger,
            asset_mint,
            share_mint,
            vault,
            idle_account,
            escrow_account,
            strategy_authority,
            strategy_account,
            adapter_own: vec![],
            user_assets,
            user_shares,
            fee_shares,
        }
    }

    pub fn create(&mut self) {
        let result = self.run(&self.create_vault_ix());
        assert!(result.program_result.is_ok(), "{:?}", result.program_result);
    }

    // ---- Accounts and time ----

    pub fn account(&self, key: &Pubkey) -> &Account {
        &self
            .accounts
            .iter()
            .find(|(k, _)| k == key)
            .expect("account")
            .1
    }

    pub fn account_mut(&mut self, key: &Pubkey) -> &mut Account {
        &mut self
            .accounts
            .iter_mut()
            .find(|(k, _)| k == key)
            .expect("account")
            .1
    }

    pub fn balance(&self, key: &Pubkey) -> u64 {
        token_amount(self.account(key))
    }

    /// Change a token balance behind the program's back: someone outside
    /// sending tokens in, or an issuer taking them out.
    pub fn set_balance(&mut self, key: Pubkey, amount: u64) {
        self.account_mut(&key).data[64..72].copy_from_slice(&amount.to_le_bytes());
    }

    pub fn advance(&mut self, slots: u64, seconds: i64) {
        self.mollusk.sysvars.clock.slot += slots;
        self.mollusk.sysvars.clock.unix_timestamp += seconds;
    }

    pub fn state(&self) -> &Vault {
        let data = &self.account(&self.vault).data;
        assert_eq!(data.len(), VAULT_LEN);
        // SAFETY: the length matches the layout, which has alignment 1.
        unsafe { Vault::from_bytes_unchecked(data) }
    }

    pub fn ticket(&self, id: u64) -> Pubkey {
        Pubkey::find_program_address(
            &[
                TICKET_SEED,
                self.vault.as_ref(),
                self.user.as_ref(),
                &id.to_le_bytes(),
            ],
            &PROGRAM_ID,
        )
        .0
    }

    /// Shares left in ticket `id`, or `None` once it is closed.
    pub fn ticket_shares(&self, id: u64) -> Option<u64> {
        let data = &self.account(&self.ticket(id)).data;
        // SAFETY: the length matches the layout, which has alignment 1.
        (data.len() == TICKET_LEN).then(|| unsafe { Ticket::from_bytes_unchecked(data) }.shares())
    }

    /// Make sure every account the instruction names exists, as an empty
    /// wallet if nothing created it yet (PDAs about to be created).
    fn ensure(&mut self, ix: &Instruction) {
        for meta in &ix.accounts {
            if !self.accounts.iter().any(|(k, _)| *k == meta.pubkey) {
                self.accounts.push((meta.pubkey, wallet(0)));
            }
        }
    }

    /// Run one instruction and fold its accounts back only if it succeeded,
    /// so a failed instruction leaves the fixture untouched, as on chain.
    pub fn run(&mut self, ix: &Instruction) -> InstructionResult {
        self.ensure(ix);
        let result = self.mollusk.process_instruction(ix, &self.accounts);
        if result.program_result.is_ok() {
            for (key, account) in &result.resulting_accounts {
                match self.accounts.iter_mut().find(|(k, _)| k == key) {
                    Some(slot) => slot.1 = account.clone(),
                    None => self.accounts.push((*key, account.clone())),
                }
            }
        }
        result
    }

    /// Run an instruction that must succeed.
    pub fn ok(&mut self, ix: &Instruction) -> InstructionResult {
        let result = self.run(ix);
        assert!(result.program_result.is_ok(), "{:?}", result.program_result);
        result
    }

    /// Run an instruction that must fail with `error`.
    pub fn fails(&mut self, ix: &Instruction, error: VaultError) {
        assert_eq!(custom_error(&self.run(ix)), Some(error as u32));
    }

    // ---- Instruction builders ----

    fn ix(&self, discriminator: u8, data: &[u8], mut accounts: Vec<AccountMeta>) -> Instruction {
        accounts.push(AccountMeta::new_readonly(EVENT_AUTHORITY, false));
        accounts.push(AccountMeta::new_readonly(PROGRAM_ID, false));
        Instruction {
            program_id: PROGRAM_ID,
            accounts,
            data: [&[discriminator], data].concat(),
        }
    }

    /// The trailing accounts that reach the adapter: the interface prefix,
    /// the adapter program, then the adapter's own accounts. The strategy
    /// authority is writable because some protocols require it of a signer.
    pub fn adapter_accounts(&self) -> Vec<AccountMeta> {
        let mut accounts = vec![
            AccountMeta::new(self.strategy_authority, false),
            AccountMeta::new(self.strategy_account, false),
            AccountMeta::new_readonly(self.asset_mint, false),
            AccountMeta::new_readonly(self.token_program, false),
            AccountMeta::new_readonly(self.adapter, false),
        ];
        accounts.extend(self.adapter_own.iter().cloned());
        accounts
    }

    /// Move an account to another address, and point every token account
    /// that named the old one as its mint or owner at the new one. Lets a
    /// test stand the vault on accounts that exist elsewhere, such as a real
    /// mint or an associated token account.
    pub fn rekey(&mut self, old: Pubkey, new: Pubkey) {
        for (key, account) in &mut self.accounts {
            if *key == old {
                *key = new;
            }
            let is_token = account.owner == TOKEN || account.owner == TOKEN_2022;
            if is_token && account.data.len() >= 165 {
                for field in [0..32, 32..64] {
                    if account.data[field.clone()] == old.to_bytes() {
                        account.data[field].copy_from_slice(new.as_ref());
                    }
                }
            }
        }
    }

    pub fn create_vault_ix(&self) -> Instruction {
        let bump =
            Pubkey::find_program_address(&[VAULT_SEED, self.share_mint.as_ref()], &PROGRAM_ID).1;
        let strategy_bump =
            Pubkey::find_program_address(&[STRATEGY_SEED, self.vault.as_ref()], &PROGRAM_ID).1;
        let data = [
            &[bump, strategy_bump][..],
            self.adapter.as_ref(),
            &self.timelock.to_le_bytes(),
            &self.config.bytes(),
        ]
        .concat();
        self.ix(
            0,
            &data,
            vec![
                AccountMeta::new(self.payer, true),
                AccountMeta::new_readonly(self.owner, true),
                AccountMeta::new(self.vault, false),
                AccountMeta::new_readonly(self.asset_mint, false),
                AccountMeta::new_readonly(self.share_mint, false),
                AccountMeta::new_readonly(self.idle_account, false),
                AccountMeta::new_readonly(self.escrow_account, false),
                AccountMeta::new_readonly(self.strategy_account, false),
                AccountMeta::new_readonly(SYSTEM, false),
            ],
        )
    }

    /// An instruction over `RoleAccounts`, signed by `authority`.
    pub fn role_ix(&self, discriminator: u8, authority: Pubkey, data: &[u8]) -> Instruction {
        self.ix(
            discriminator,
            data,
            vec![
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new(self.vault, false),
            ],
        )
    }

    pub fn submit_config_ix(&self, authority: Pubkey, config: &ConfigArgs) -> Instruction {
        self.role_ix(1, authority, &config.bytes())
    }

    pub fn execute_config_ix(&self, authority: Pubkey) -> Instruction {
        self.role_ix(2, authority, &[])
    }

    pub fn transfer_ownership_ix(&self, authority: Pubkey, new_owner: Pubkey) -> Instruction {
        self.role_ix(3, authority, new_owner.as_ref())
    }

    pub fn accept_ownership_ix(&self, authority: Pubkey) -> Instruction {
        self.role_ix(4, authority, &[])
    }

    pub fn wind_down_ix(&self, authority: Pubkey) -> Instruction {
        self.role_ix(5, authority, &[])
    }

    pub fn set_paused_ix(&self, authority: Pubkey, paused: bool) -> Instruction {
        self.role_ix(10, authority, &[paused as u8])
    }

    pub fn write_off_ix(&self, authority: Pubkey, value: u64) -> Instruction {
        self.role_ix(11, authority, &value.to_le_bytes())
    }

    /// Allocate or deallocate (`discriminator` 20 or 21), with the adapter
    /// accounts after the fixed ones.
    fn manager_ix(&self, discriminator: u8, amount: u64) -> Instruction {
        let mut ix = self.ix(
            discriminator,
            &amount.to_le_bytes(),
            vec![
                AccountMeta::new_readonly(self.manager, true),
                AccountMeta::new(self.vault, false),
                AccountMeta::new(self.idle_account, false),
            ],
        );
        ix.accounts.extend(self.adapter_accounts());
        ix
    }

    pub fn allocate_ix(&self, amount: u64) -> Instruction {
        self.manager_ix(20, amount)
    }

    pub fn deallocate_ix(&self, amount: u64) -> Instruction {
        self.manager_ix(21, amount)
    }

    pub fn deposit_ix(&self, assets: u64, min_shares_out: u64) -> Instruction {
        self.deposit_with_permit_ix(assets, min_shares_out, &[])
    }

    /// A deposit carrying `permit`, as `Fixture::permit` builds one.
    pub fn deposit_with_permit_ix(
        &self,
        assets: u64,
        min_shares_out: u64,
        permit: &[u8],
    ) -> Instruction {
        let data = [
            &assets.to_le_bytes()[..],
            &min_shares_out.to_le_bytes(),
            permit,
        ]
        .concat();
        self.ix(
            30,
            &data,
            vec![
                AccountMeta::new_readonly(self.user, true),
                AccountMeta::new(self.vault, false),
                AccountMeta::new_readonly(self.asset_mint, false),
                AccountMeta::new(self.share_mint, false),
                AccountMeta::new(self.idle_account, false),
                AccountMeta::new(self.user_assets, false),
                AccountMeta::new(self.user_shares, false),
                AccountMeta::new_readonly(self.token_program, false),
                AccountMeta::new_readonly(self.share_program, false),
            ],
        )
    }

    pub fn request_redeem_ix(&self, shares: u64, id: u64) -> Instruction {
        let ticket = self.ticket(id);
        let bump = Pubkey::find_program_address(
            &[
                TICKET_SEED,
                self.vault.as_ref(),
                self.user.as_ref(),
                &id.to_le_bytes(),
            ],
            &PROGRAM_ID,
        )
        .1;
        let data = [&shares.to_le_bytes()[..], &id.to_le_bytes(), &[bump]].concat();
        self.ix(
            31,
            &data,
            vec![
                AccountMeta::new(self.payer, true),
                AccountMeta::new_readonly(self.user, true),
                AccountMeta::new(self.vault, false),
                AccountMeta::new(ticket, false),
                AccountMeta::new(self.share_mint, false),
                AccountMeta::new(self.user_shares, false),
                AccountMeta::new(self.escrow_account, false),
                AccountMeta::new(self.idle_account, false),
                AccountMeta::new_readonly(self.asset_mint, false),
                AccountMeta::new(self.user_assets, false),
                AccountMeta::new_readonly(self.share_program, false),
                AccountMeta::new_readonly(self.token_program, false),
                AccountMeta::new_readonly(SYSTEM, false),
            ],
        )
    }

    pub fn cancel_redeem_ix(&self, id: u64) -> Instruction {
        self.ix(
            32,
            &[],
            vec![
                AccountMeta::new_readonly(self.user, true),
                AccountMeta::new(self.payer, false),
                AccountMeta::new_readonly(self.vault, false),
                AccountMeta::new(self.ticket(id), false),
                AccountMeta::new_readonly(self.share_mint, false),
                AccountMeta::new(self.escrow_account, false),
                AccountMeta::new(self.user_shares, false),
                AccountMeta::new_readonly(self.share_program, false),
            ],
        )
    }

    /// Simulate: anyone asks the adapter what the strategy is worth.
    pub fn simulate_ix(&self) -> Instruction {
        let mut ix = self.ix(
            40,
            &[],
            vec![
                AccountMeta::new(self.vault, false),
                AccountMeta::new_readonly(self.idle_account, false),
            ],
        );
        ix.accounts.extend(self.adapter_accounts());
        ix
    }

    /// Fulfil ticket `id`, with the adapter accounts so a shortfall can be pulled.
    pub fn fulfil_ix(&self, id: u64) -> Instruction {
        self.fulfil_with_permit_ix(id, &[])
    }

    /// A fulfilment carrying `permit`, as `Fixture::permit` builds one.
    pub fn fulfil_with_permit_ix(&self, id: u64, permit: &[u8]) -> Instruction {
        let data = [&[!permit.is_empty() as u8][..], permit].concat();
        let mut ix = self.ix(
            41,
            &data,
            vec![
                AccountMeta::new(self.vault, false),
                AccountMeta::new(self.ticket(id), false),
                AccountMeta::new(self.payer, false),
                AccountMeta::new(self.escrow_account, false),
                AccountMeta::new(self.share_mint, false),
                AccountMeta::new(self.idle_account, false),
                AccountMeta::new_readonly(self.asset_mint, false),
                AccountMeta::new(self.user_assets, false),
                AccountMeta::new_readonly(self.token_program, false),
                AccountMeta::new_readonly(self.share_program, false),
            ],
        );
        ix.accounts.extend(self.adapter_accounts());
        ix
    }

    /// What an authority signs off chain to let `subject` act until
    /// `expires_at`: `[expires_at][signature]`, ready to append to an instruction.
    pub fn permit(
        &self,
        signer: &SigningKey,
        kind: u8,
        subject: &Pubkey,
        expires_at: i64,
    ) -> Vec<u8> {
        self.permit_with_nonce(signer, kind, subject, 0, expires_at)
    }

    /// A fulfil permit for ticket `id`, as it stands now: it names the
    /// ticket's nonce, so it is good for that ticket only.
    pub fn fulfil_permit(&self, signer: &SigningKey, id: u64, expires_at: i64) -> Vec<u8> {
        let ticket = self.ticket(id);
        // SAFETY: the account is a ticket: its length matches the layout,
        // which has alignment 1.
        let nonce = unsafe { Ticket::from_bytes_unchecked(&self.account(&ticket).data) }.nonce();
        self.permit_with_nonce(signer, PERMIT_FULFIL, &ticket, nonce, expires_at)
    }

    /// A permit naming `nonce`: zero for a deposit, the ticket's for a fulfilment.
    pub fn permit_with_nonce(
        &self,
        signer: &SigningKey,
        kind: u8,
        subject: &Pubkey,
        nonce: u64,
        expires_at: i64,
    ) -> Vec<u8> {
        use ed25519_dalek::Signer;
        let message = vault_core::permit_message(
            kind,
            &self.vault.to_bytes(),
            &subject.to_bytes(),
            nonce,
            expires_at,
        );
        [
            &expires_at.to_le_bytes()[..],
            &signer.sign(&message).to_bytes(),
        ]
        .concat()
    }

    /// The current Unix timestamp on the fixture's clock.
    pub fn now(&self) -> i64 {
        self.mollusk.sysvars.clock.unix_timestamp
    }

    pub fn collect_fees_ix(&self) -> Instruction {
        self.ix(
            42,
            &[],
            vec![
                AccountMeta::new(self.vault, false),
                AccountMeta::new(self.share_mint, false),
                AccountMeta::new(self.fee_shares, false),
                AccountMeta::new_readonly(self.share_program, false),
            ],
        )
    }

    /// Run a view of the read interface and decode its return data.
    pub fn view(&mut self, discriminator: [u8; 8], data: &[u8]) -> u64 {
        let ix = Instruction {
            program_id: PROGRAM_ID,
            accounts: vec![AccountMeta::new_readonly(self.vault, false)],
            data: [&discriminator[..], data].concat(),
        };
        let result = self.ok(&ix);
        u64::from_le_bytes(result.return_data.as_slice().try_into().unwrap())
    }

    /// Reprice the vault: simulate, and let the adapter value the position itself.
    pub fn reprice(&mut self) -> InstructionResult {
        self.ok(&self.simulate_ix())
    }
}
