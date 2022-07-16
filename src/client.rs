//! Holds the _largely internal_ representation of a client, however it's exposed as the core
//! state machine is exposed through [`Client`].

use color_eyre::{
    eyre::{eyre, WrapErr},
    Result,
};
use fnv::FnvHashMap;
use serde::{
    ser::{Error, SerializeStruct},
    Serialize,
};
use tracing::{instrument, warn};

use std::{collections::hash_map::Entry, fmt};

use crate::transaction::{Transaction, TransactionType};
use crate::Amount;

/// Holds all transactional data related to a specific client.
///
/// The public API is minimal in order to enforce the state machine held
/// internally within it, any changes to the API/internals should be carefully thought
/// through to ensure they don't break the machine.
///
/// ## Examples
///
/// ```
/// use lib::client::Client;
/// use lib::transaction::TransactionType;
/// use lib::Amount;
///
/// let mut client = Client::new(1);
///
/// if let Err(e) = client.process_transaction(1, TransactionType::Deposit, Some(Amount::try_from(10f32).unwrap())) {
///     eprintln!("Error occurred {}", e);
/// }
/// ```
pub struct Client {
    /// The Unique ID associated with this client
    pub id: u16,
    status: AccountStatus,
    transaction_log: FnvHashMap<u32, Option<Transaction>>,
    held: Amount,
    available: Amount,
}

/// An enum representation of the status of the account
///
/// If an account is frozen, no further transactions can take place
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountStatus {
    Active,
    Frozen,
}

impl Client {
    pub fn new(client_id: u16) -> Self {
        Self {
            id: client_id,
            status: AccountStatus::Active,
            transaction_log: FnvHashMap::with_capacity_and_hasher(5, Default::default()),
            available: Amount::default(),
            held: Amount::default(),
        }
    }

    /// A getter method identifying whether this client's account is locked
    pub fn is_locked(&self) -> bool {
        self.status == AccountStatus::Frozen
    }

    /// A getter method used to calculate the total funds for this client
    pub fn total_funds(&self) -> Result<f32> {
        (self.available + self.held)
            .try_into()
            .wrap_err("unexpected error occurred when attempting to calculate total funds")
    }

    /// A getter method used to retrieve the available funds for this client
    pub fn available_funds(&self) -> Result<f32> {
        self.available
            .try_into()
            .wrap_err("unexpected error occurred when attempting to calculate available funds")
    }

    /// A getter method used to retrieve the held funds for this client
    pub fn held_funds(&self) -> Result<f32> {
        self.held
            .try_into()
            .wrap_err("unexpected error occurred when attempting to calculate held funds")
    }

    /// Processes the incoming transaction
    ///
    /// ## Errors
    ///
    /// This function will error if:
    /// 1. The account status of the client is [`AccountStatus::Frozen`]
    /// 2. An unexpected error occurs
    ///
    /// An error from this function indicates that processing should stop for this client
    ///
    /// ## Ignores
    ///
    /// This function will ignore _and return `Ok(())`_ for any invalid transactions, whether that be due to:
    /// - Invalid state transtions
    /// - Not enough funds to carry out a withdrawal
    /// - Invalid data (eg. A deposit or withdrawal with no amount)
    #[instrument(level = "debug", skip(self, amount), fields(client_id = %self.id), err)]
    pub fn process_transaction(
        &mut self,
        transaction_id: u32,
        transaction_type: TransactionType,
        amount: Option<Amount>,
    ) -> Result<()> {
        if self.status == AccountStatus::Frozen {
            warn!("unable to carry out transaction as account is frozen");
            // TODO - Make this a matchable enum
            return Err(eyre!(
                "[FROZEN_ACCOUNT]: unable to carry out transaction when the account is frozen"
            ));
        }

        // In this case we have a brand new transaction we've not seen before
        if !self.transaction_log.contains_key(&transaction_id) {
            match transaction_type {
                TransactionType::Deposit if amount.is_some() => {
                    self.deposit(transaction_id, amount.unwrap())
                }
                TransactionType::Withdrawal if amount.is_some() => {
                    self.withdraw(transaction_id, amount.unwrap())
                }
                TransactionType::Deposit | TransactionType::Withdrawal => warn!(
                    "unable to process transition type {:?} when no amount is provided",
                    transaction_type
                ),
                _ => {
                    warn!("Unable to process transaction type {:?} as transaction id {} does not exist for client {}", transaction_type, transaction_id, self.id);
                }
            }
            return Ok(());
        }

        match self.transaction_log.remove(&transaction_id) {
            // We currently have a transaction stored under this id
            Some(Some(trx)) => {
                match trx.transition(transaction_type) {
                    Ok(state_change) => match state_change {
                        Transaction::Dispute { amount } => self.dispute(transaction_id, amount),
                        Transaction::Resolve { amount } => self.resolve(transaction_id, amount),
                        Transaction::Chargeback { amount } => {
                            self.chargeback(amount);
                            return Err(eyre!("[FROZEN_ACCOUNT] this client account '{}' has been frozen, no further transactions can occur", self.id));
                        },
                        _ => return Err(eyre!("an unexpected error occured, it should not be possible to make this transition"))
                    }
                    // In this case, an invalid state transition has occurred
                    // so we ignore it
                    Err(_) => {
                        self.transaction_log.insert(transaction_id, Some(trx));
                    }
                }
            }
            // A transaction with this id has already been resolved in some manner
            // - This handles duplicate transaction ids
            Some(None) => {
                warn!(
                    "attempted to process transaction id: {} which has already been processed",
                    transaction_id
                );
                self.transaction_log.insert(transaction_id, None);
            }

            None => unreachable!("this is handled by the contains_key check above"),
        }
        Ok(())
    }

    /// This will update the internally held totals on the funds and insert an entry in the
    /// transaction log
    ///
    /// If this provided transaction id has already been processed, this will ignore it
    fn deposit(&mut self, transaction_id: u32, amount: Amount) {
        match self.transaction_log.entry(transaction_id) {
            Entry::Occupied(_) => {
                warn!(
                    "we have already processed transaction id {}, therefore we're ignoring this",
                    transaction_id
                );
            }
            Entry::Vacant(v) => {
                v.insert(Some(Transaction::Deposit { amount }));
                self.available += amount;
            }
        }
    }

    /// This will update the internally held totals on the funds and insert an entry in the
    /// transaction log
    ///
    /// If this provided transaction id has already been processed, this will ignore it
    fn withdraw(&mut self, transaction_id: u32, amount: Amount) {
        match self.transaction_log.entry(transaction_id) {
            Entry::Occupied(_) => {
                warn!(
                    "we have already processed transaction id {}, therefore we're ignoring this",
                    transaction_id
                );
            }
            Entry::Vacant(v) => {
                if self.available >= amount {
                    self.available -= amount;
                    v.insert(None);
                } else {
                    warn!("unable to withdraw as the account does not have enough available funds")
                }
            }
        }
    }

    fn dispute(&mut self, transaction_id: u32, amount: Amount) {
        self.available -= amount;
        self.held += amount;

        self.transaction_log
            .insert(transaction_id, Some(Transaction::Dispute { amount }));
    }

    fn resolve(&mut self, transaction_id: u32, amount: Amount) {
        self.held -= amount;
        self.available += amount;

        // This is an optimization based off the **Valid State Transitions** assumption
        // in the readme.
        //
        // If we enter this state, this transaction id can no longer be modified, therefore we can
        // completely remove the associated data. However we keep the transaction id so we don't
        // re-process if it gets passed through again
        self.transaction_log.insert(transaction_id, None);
    }

    fn chargeback(&mut self, amount: Amount) {
        self.held -= amount;
        self.status = AccountStatus::Frozen;

        // This is an optimization
        //
        // - If we enter this state, we have frozen the account
        // therefore we don't actually have to keep any transactions in the log
        // as we won't be processing any more transactions for this client so we can free up this memory
        // - This optimization is tied to the fact that this is a CLI app that runs once
        // - In a real life scenario ie. API, we could still make this optimization, but
        // we would need a way to re-populate the transaction log should the account become
        // unfrozen.
        self.transaction_log.clear();
    }
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Client")
            .field("id", &self.id)
            .field("status", &self.status)
            .field("transaction_log_length", &self.transaction_log.len())
            .finish()
    }
}

impl Serialize for Client {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("Client", 5)?;
        state.serialize_field("client", &self.id)?;
        state.serialize_field(
            "available",
            &self
                .available_funds()
                .map_err(|e| Error::custom(e.to_string()))?,
        )?;
        state.serialize_field(
            "held",
            &self
                .held_funds()
                .map_err(|e| Error::custom(e.to_string()))?,
        )?;
        state.serialize_field(
            "total",
            &self
                .total_funds()
                .map_err(|e| Error::custom(e.to_string()))?,
        )?;
        state.serialize_field("locked", &self.is_locked())?;
        state.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use pretty_assertions::assert_eq;
    use serde::Deserialize;

    const ALLOWABLE_ERROR: f32 = 0.000049;

    impl Clone for Client {
        fn clone(&self) -> Self {
            Self {
                id: self.id,
                status: self.status,
                transaction_log: self.transaction_log.clone(),
                held: self.held,
                available: self.available,
            }
        }
    }

    #[test]
    fn can_be_serialized() -> Result<()> {
        let client = client_with_state();
        let mut result = vec![];
        {
            let mut writer = csv::Writer::from_writer(&mut result);
            writer.serialize(&client)?;
            writer.flush()?;
        }

        #[derive(Debug, Deserialize)]
        struct Test {
            client: u16,
            available: f32,
            held: f32,
            total: f32,
            locked: bool,
        }

        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(true)
            .trim(csv::Trim::All)
            .from_reader(&*result);
        for result in rdr.deserialize::<Test>() {
            let res = result?;
            assert_eq!(res.client, client.id);
            assert_eq!(res.available, client.available_funds()?);
            assert_eq!(res.held, client.held_funds()?);
            assert_eq!(res.total, client.total_funds()?);
            assert_eq!(res.locked, client.is_locked());
        }
        Ok(())
    }

    #[test]
    fn errors_if_the_account_is_frozen() -> Result<()> {
        let mut client = client_with_state();
        client.status = AccountStatus::Frozen;
        let tx_id = 1;
        for tx in &[
            TransactionType::Deposit,
            TransactionType::Withdrawal,
            TransactionType::Dispute,
            TransactionType::Resolve,
            TransactionType::Chargeback,
        ] {
            let res = client.process_transaction(tx_id, *tx, None);
            assert!(
                res.is_err(),
                "if the account is frozen we should always error"
            );
        }
        Ok(())
    }

    #[test]
    fn handles_deposit_with_a_new_transaction_id() -> Result<()> {
        let mut client = Client::new(1);
        let tx_id = 1;
        let tx_amt = 1.23f32;
        client.process_transaction(tx_id, TransactionType::Deposit, Some(Amount::new(tx_amt)?))?;
        assert_eq!(
            client.transaction_log.len(),
            1,
            "transaction log has a size of 1"
        );
        assert_eq!(client.available_funds()?, tx_amt);
        assert_eq!(client.held_funds()?, 0f32);
        let log_data = client.transaction_log.get(&tx_id);
        assert!(
            log_data.is_some(),
            "the transaction id should be present in the log"
        );
        assert!(
            log_data.unwrap().is_some(),
            "the nested transaction id should be some"
        );
        let is_deposit_type = matches!(log_data.unwrap().unwrap(), Transaction::Deposit { .. });
        assert!(is_deposit_type, "the transaction should be of type deposit");
        Ok(())
    }

    #[test]
    fn handles_a_deposit_with_a_duplicate_transaction_id() -> Result<()> {
        let before = client_with_state();
        let mut after = before.clone();
        let tx_id = 1;
        let tx_amt = 1.23f32;
        after.process_transaction(tx_id, TransactionType::Deposit, Some(Amount::new(tx_amt)?))?;
        check_has_not_mutated_state(before, after)?;

        Ok(())
    }

    #[test]
    fn handles_a_withdrawal_with_a_new_transaction_id() -> Result<()> {
        let before = client_with_state();
        let mut after = before.clone();
        let tx_id = 3;
        let tx_amt = 1.23f32;
        after.process_transaction(
            tx_id,
            TransactionType::Withdrawal,
            Some(Amount::new(tx_amt)?),
        )?;
        assert_eq!(
            after.transaction_log.len(),
            3,
            "the withdrawal transaction should be added to the log"
        );
        assert_eq!(after.available_funds()?, before.available_funds()? - tx_amt);
        assert_eq!(
            before.held_funds()?,
            after.held_funds()?,
            "held funds should not change"
        );
        let log_data = after.transaction_log.get(&tx_id);
        assert!(
            log_data.is_some(),
            "the transaction id should be present in the log"
        );
        assert!(
            log_data.unwrap().is_none(),
            "we don't need to store any data for withdrawals other than the transaction id"
        );
        Ok(())
    }

    #[test]
    fn handles_a_duplicate_withdrawal_request() -> Result<()> {
        let mut before = client_with_state();
        let tx_id = 3;
        let tx_amt = 1.23f32;
        before.process_transaction(
            tx_id,
            TransactionType::Withdrawal,
            Some(Amount::new(tx_amt)?),
        )?;
        let mut after = before.clone();
        after.process_transaction(
            tx_id,
            TransactionType::Withdrawal,
            Some(Amount::new(tx_amt)?),
        )?;
        check_has_not_mutated_state(before, after)?;
        Ok(())
    }

    #[test]
    fn handles_a_dispute_on_a_deposit() -> Result<()> {
        let mut client = client_with_state();
        let tx_id = 1;
        let previous_available_balance = client.available_funds()?;
        let previous_held_balance = client.held_funds()?;
        client.process_transaction(tx_id, TransactionType::Dispute, None)?;
        assert_eq!(
            client.available_funds()?,
            0f32,
            "available funds should be reduced to zero"
        );
        assert!(
            (client.held_funds()? - (previous_held_balance + previous_available_balance)).abs()
                < ALLOWABLE_ERROR,
            "all funds should now be held"
        );

        let log_data = client.transaction_log.get(&tx_id);
        assert!(
            log_data.is_some(),
            "the transaction id should be present in the log"
        );
        assert!(
            log_data.unwrap().is_some(),
            "the nested transaction id should be some"
        );
        let tx_type = matches!(log_data.unwrap().unwrap(), Transaction::Dispute { .. });
        assert!(tx_type, "the transaction should be of type dispute");
        Ok(())
    }

    #[test]
    fn should_ignore_duplicate_dispute_requests() -> Result<()> {
        let mut before = client_with_state();
        let tx_id = 1;
        before.process_transaction(tx_id, TransactionType::Dispute, None)?;
        let mut after = before.clone();
        after.process_transaction(tx_id, TransactionType::Dispute, None)?;

        check_has_not_mutated_state(before, after)?;
        Ok(())
    }

    #[test]
    fn handles_a_resolve_on_a_dispute() -> Result<()> {
        let mut client = client_with_state();
        let tx_id = 2;
        let previous_available_funds = client.available_funds()?;
        let previous_held_balance = client.held_funds()?;
        client.process_transaction(tx_id, TransactionType::Resolve, None)?;
        assert!(
            (client.available_funds()? - (previous_held_balance + previous_available_funds)).abs()
                < ALLOWABLE_ERROR,
            "available funds should be back to the full amount"
        );
        assert_eq!(client.held_funds()?, 0f32, "held funds should be back to 0");

        let log_data = client.transaction_log.get(&tx_id);
        assert!(
            log_data.is_some(),
            "the transaction id should still be present in the log"
        );
        assert!(
            log_data.unwrap().is_none(),
            "we no longer need to hold transaction data"
        );
        Ok(())
    }

    #[test]
    fn should_ignore_duplicate_resolve_requests() -> Result<()> {
        let mut before = client_with_state();
        let tx_id = 2;
        before.process_transaction(tx_id, TransactionType::Resolve, None)?;
        let mut after = before.clone();
        after.process_transaction(tx_id, TransactionType::Resolve, None)?;

        check_has_not_mutated_state(before, after)?;
        Ok(())
    }

    #[test]
    fn handles_a_chargeback_on_a_dispute() -> Result<()> {
        let mut client = client_with_state();
        let tx_id = 2;
        let previous_available_funds = client.available_funds()?;
        let err = client.process_transaction(tx_id, TransactionType::Chargeback, None);
        assert!(
            err.is_err(),
            "expected a failure from a publish transaction as the account is locked"
        );
        assert_eq!(
            client.available_funds()?,
            previous_available_funds,
            "available funds should not be touched when a chargeback occurs"
        );
        assert_eq!(
            client.held_funds()?,
            0f32,
            "held funds should reduce when a chargeback occurs"
        );

        assert_eq!(
            client.transaction_log.len(),
            0,
            "when a chargeback occurs we can clear the transaction log"
        );
        assert_eq!(
            client.status,
            AccountStatus::Frozen,
            "the client should be frozen when a chargeback occurs"
        );
        Ok(())
    }

    #[test]
    fn handles_illegal_transitions_from_deposit() -> Result<()> {
        let before = client_with_state();
        let tx_id = 1;
        let tx_amt = 1.23f32;

        for transition in &[
            TransactionType::Withdrawal,
            TransactionType::Resolve,
            TransactionType::Chargeback,
        ] {
            let mut after = before.clone();
            after.process_transaction(tx_id, *transition, Some(Amount::new(tx_amt)?))?;
            check_has_not_mutated_state(before.clone(), after)?;
        }
        Ok(())
    }

    #[test]
    fn handles_illegal_transitions_from_withdrawal() -> Result<()> {
        let mut before = client_with_state();
        let tx_id = 3;
        let tx_amt = 1.23f32;
        before.process_transaction(
            tx_id,
            TransactionType::Withdrawal,
            Some(Amount::new(tx_amt)?),
        )?;

        for transition in &[
            TransactionType::Deposit,
            TransactionType::Dispute,
            TransactionType::Resolve,
            TransactionType::Chargeback,
        ] {
            let mut after = before.clone();
            after.process_transaction(tx_id, *transition, Some(Amount::new(tx_amt)?))?;
            check_has_not_mutated_state(before.clone(), after)?;
        }
        Ok(())
    }

    #[test]
    fn handles_illegal_transitions_from_dispute() -> Result<()> {
        let before = client_with_state();
        let tx_id = 2;
        let tx_amt = 1.23f32;

        for transition in &[TransactionType::Deposit, TransactionType::Withdrawal] {
            let mut after = before.clone();
            after.process_transaction(tx_id, *transition, Some(Amount::new(tx_amt)?))?;
            check_has_not_mutated_state(before.clone(), after)?;
        }
        Ok(())
    }

    #[test]
    fn handles_illegal_transitions_from_resolve() -> Result<()> {
        let mut before = client_with_state();
        let tx_id = 2;
        let tx_amt = 1.23f32;
        before.process_transaction(tx_id, TransactionType::Resolve, None)?;

        for transition in &[
            TransactionType::Deposit,
            TransactionType::Withdrawal,
            TransactionType::Dispute,
            TransactionType::Chargeback,
        ] {
            let mut after = before.clone();
            after.process_transaction(tx_id, *transition, Some(Amount::new(tx_amt)?))?;
            check_has_not_mutated_state(before.clone(), after)?;
        }
        Ok(())
    }

    fn client_with_state() -> Client {
        let mut log: FnvHashMap<u32, Option<Transaction>> = Default::default();
        let available = Amount::new(20.32f32).unwrap();
        let held = Amount::new(3.14923f32).unwrap();
        log.insert(1, Some(Transaction::Deposit { amount: available }));
        log.insert(2, Some(Transaction::Dispute { amount: held }));
        Client {
            id: 1,
            available,
            held,
            status: AccountStatus::Active,
            transaction_log: log,
        }
    }

    fn check_has_not_mutated_state(before: Client, after: Client) -> Result<()> {
        assert_eq!(
            before.available_funds()?,
            after.available_funds()?,
            "expected that available funds would not change"
        );
        assert_eq!(
            before.held_funds()?,
            after.held_funds()?,
            "expected that held funds would not change"
        );
        assert_eq!(
            before.total_funds()?,
            after.total_funds()?,
            "expected that total funds would not change"
        );
        assert_eq!(
            before.is_locked(),
            after.is_locked(),
            "expected that account status would not change"
        );
        let rhs_log = &after.transaction_log;
        for (k, v) in before.transaction_log.iter() {
            let rhs_value = rhs_log.get(k);
            assert!(
                rhs_value.is_some(),
                "expected the transaction log to have an entry for transaction {}",
                k
            );
            let rhs_value = rhs_value.unwrap();
            assert_eq!(
                v, rhs_value,
                "expected transaction state for transaction {} to not have changed",
                k
            );
        }
        Ok(())
    }
}
