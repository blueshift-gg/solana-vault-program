//! A vault behind the custody adapter: the default vault of the end-to-end
//! tests. The test plays the people around it: the oracle signing prices and
//! the custodian sending funds back.

use crate::*;
use custody_adapter::constants::*;
use custody_adapter::state::report_message;
use ed25519_dalek::Signer;
use mollusk_svm::program::create_program_account_loader_v3;

pub const CUSTODY_ID: Pubkey = Pubkey::new_from_array(custody_adapter::ID);

/// The oracle's key in these tests.
pub fn oracle() -> SigningKey {
    SigningKey::from_bytes(&[11; 32])
}

pub fn key(signer: &SigningKey) -> Pubkey {
    Pubkey::new_from_array(signer.verifying_key().to_bytes())
}

pub struct Custodied {
    pub f: Fixture,
    pub custody: Pubkey,
    /// The custodian's token account: the only place funds are sent.
    pub destination: Pubkey,
    /// The custody's own token account: where the custodian sends funds back.
    pub return_account: Pubkey,
}

impl Custodied {
    /// A vault over an SPL Token asset with its custody book open.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let mut c = Self::setup(TOKEN);
        c.open();
        c
    }

    /// Every account the vault and its custody need, with neither created
    /// yet, so a test can change the configuration or the accounts first.
    pub fn setup(token_program: Pubkey) -> Self {
        Self::setup_with_shares(token_program, TOKEN)
    }

    /// `setup`, with the share mint under `share_program`.
    pub fn setup_with_shares(token_program: Pubkey, share_program: Pubkey) -> Self {
        let mut f = Fixture::setup_with_shares(CUSTODY_ID, token_program, share_program);
        f.mollusk.add_program(&CUSTODY_ID, "custody_adapter");
        let custody = Pubkey::find_program_address(
            &[CUSTODY_SEED, f.strategy_authority.as_ref()],
            &CUSTODY_ID,
        )
        .0;
        let [destination, return_account, custodian] =
            core::array::from_fn(|_| Pubkey::new_unique());
        f.accounts.extend([
            (CUSTODY_ID, create_program_account_loader_v3(&CUSTODY_ID)),
            (
                destination,
                token_account(&token_program, &f.asset_mint, &custodian, 0),
            ),
            (
                return_account,
                token_account(&token_program, &f.asset_mint, &custody, 0),
            ),
        ]);
        f.adapter_own = idl_accounts(
            "custody",
            &[
                ("custody", custody),
                ("returnAccount", return_account),
                ("destination", destination),
            ],
        );
        Self {
            f,
            custody,
            destination,
            return_account,
        }
    }

    /// Create the vault and open its book.
    pub fn open(&mut self) {
        self.f.create();
        self.f.ok(&self.initialize_ix(self.f.owner));
    }

    pub fn initialize_ix(&self, owner: Pubkey) -> Instruction {
        let mut ix = idl_instruction(
            "custody",
            "initialize",
            &[
                ("payer", self.f.payer),
                ("owner", owner),
                ("vault", self.f.vault),
                ("custody", self.custody),
                ("destination", self.destination),
                ("returnAccount", self.return_account),
            ],
        );
        ix.data.extend(key(&oracle()).as_ref());
        ix
    }

    pub fn set_oracle_ix(&self, owner: Pubkey, oracle: Pubkey) -> Instruction {
        let mut ix = idl_instruction(
            "custody",
            "setOracle",
            &[
                ("owner", owner),
                ("vault", self.f.vault),
                ("custody", self.custody),
            ],
        );
        ix.data.extend(oracle.as_ref());
        ix
    }

    /// `Simulate` delivering a price of `price` per unit (1.0 is
    /// `PRICE_SCALE`), signed by `signer` and good for `validity` seconds.
    pub fn price_ix(&self, signer: &SigningKey, price: u64, validity: i64) -> Instruction {
        let expires_at = self.f.now() + validity;
        let message = report_message(
            &self.f.strategy_authority.to_bytes(),
            self.book().book(),
            price,
            expires_at,
        );
        let mut ix = self.f.simulate_ix();
        ix.data.extend(price.to_le_bytes());
        ix.data.extend(expires_at.to_le_bytes());
        ix.data.extend(signer.sign(&message).to_bytes());
        ix
    }

    /// The custody account, as the adapter stores it.
    pub fn book(&self) -> &custody_adapter::state::Custody {
        custody_adapter::state::Custody::from_bytes(&self.f.account(&self.custody).data)
            .expect("custody")
    }

    /// Units of the off-chain position the book holds.
    pub fn units(&self) -> u64 {
        self.book().units()
    }

    /// The oracle prices the book so that what is off chain is worth `value`,
    /// and the vault reprices. The price is good for a century, so a test
    /// about something else never trips over its expiry.
    pub fn value(&mut self, value: u64) -> InstructionResult {
        let price = (value as u128 * PRICE_SCALE as u128 / self.units().max(1) as u128) as u64;
        // Each report outlives the last, as a newer one must.
        let stored =
            custody_adapter::state::Custody::from_bytes(&self.f.account(&self.custody).data)
                .expect("custody")
                .price_expires_at();
        let validity = (100 * YEAR).max(stored + 1 - self.f.now());
        let ix = self.price_ix(&oracle(), price, validity);
        self.f.ok(&ix)
    }

    /// The custodian sends `amount` back: a plain transfer to the return account.
    pub fn send_back(&mut self, amount: u64) {
        let held = self.f.balance(&self.destination);
        self.f
            .set_balance(self.destination, held.saturating_sub(amount));
        let balance = self.f.balance(&self.return_account);
        self.f.set_balance(self.return_account, balance + amount);
    }
}
