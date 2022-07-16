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

use crate::{Amount, Transaction, TransactionType};

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
/// use lib::TransactionType;
/// use lib::Amount;
///
/// let mut client = Client::new(1);
///
/// if let Err(e) = client.publish_transaction(1, TransactionType::Deposit, Some(Amount::try_from(10f32).unwrap())) {
///     eprintln!("Error occurred {}", e);
/// }
/// ```
pub struct Client {
    id: u16,
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

    /// A getter method used for the [`Serialize`] implementation
    fn is_locked(&self) -> bool {
        self.status == AccountStatus::Frozen
    }

    /// A getter method used for the [`Serialize`] implementation
    fn total_funds(&self) -> Result<f32> {
        (self.available + self.held)
            .try_into()
            .wrap_err("unexpected error occurred when attempting to calculate total funds")
    }

    /// A getter method used for the [`Serialize`] implementation
    fn available_funds(&self) -> Result<f32> {
        self.available
            .try_into()
            .wrap_err("unexpected error occurred when attempting to calculate available funds")
    }

    /// A getter method used for the [`Serialize`] implementation
    fn held_funds(&self) -> Result<f32> {
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
    /// 2. This client has already processed this transaction_id and the transaction type is
    ///    `Deposit` OR `Withdrawal`.
    ///
    /// ## Ignore
    /// 1. It will ignore any invalid state transitions - _this will handle duplicate transactions
    ///    for the other transaction types._
    ///
    #[instrument(level = "debug", skip(self, amount), fields(client_id = %self.id), err)]
    pub fn publish_transaction(
        &mut self,
        transaction_id: u32,
        transaction_type: TransactionType,
        amount: Option<Amount>,
    ) -> Result<()> {
        if self.status == AccountStatus::Frozen {
            warn!("unable to carry out transaction as account is frozen");
            return Err(eyre!(
                "unable to carry out transaction when the account is frozen"
            ));
        }

        match self.transaction_log.remove(&transaction_id) {
            Some(Some(trx)) => {
                match trx.transition(transaction_type) {
                    Ok(state_change) => match state_change {
                        Transaction::Dispute { amount } => self.dispute(transaction_id, amount),
                        Transaction::Resolve { amount } => self.resolve(transaction_id, amount),
                        Transaction::Chargeback { amount } => self.chargeback(amount),
                        _ => Err(eyre!("an unexpected error occured, it should not be possible to make this transition"))
                    }
                    Err(e) => {
                        self.transaction_log.insert(transaction_id, Some(trx));
                        Err(e)
                    }
                }
            }
            // This transaction has already been resolved in some manner
            Some(None) => {
                let msg = format!("attempted to process transaction id: {} which has already been processed", transaction_id);
                warn!("{}", &msg);
                self.transaction_log.insert(transaction_id, None);
                Err(eyre!(msg))
            }

            // This is a brand new transaction
            None => match transaction_type {
                TransactionType::Deposit if amount.is_some() => {
                    self.deposit(transaction_id, amount.unwrap())
                }
                TransactionType::Withdrawal if amount.is_some() => {
                    self.withdraw(transaction_id, amount.unwrap())
                }
                TransactionType::Deposit | TransactionType::Withdrawal => Err(eyre!(
                    "unable to process transition type {:?} when no amount is provided",
                    transaction_type
                )),
                _ => Err(eyre!("Unable to process transaction type {:?} as transaction id: {} does not exist for client {}", transaction_type, transaction_id, self.id))
            },
        }
    }

    /// Handles a deposit transaction for this client_id
    ///
    /// It will error if the provided transaction_id has been seen before
    fn deposit(&mut self, transaction_id: u32, amount: Amount) -> Result<()> {
        match self.transaction_log.entry(transaction_id) {
            Entry::Occupied(_) => {
                return Err(eyre!(
                    "we have already processed transaction id {}",
                    transaction_id
                ))
            }
            Entry::Vacant(v) => {
                v.insert(Some(Transaction::Deposit { amount }));
                self.available += amount;
            }
        }
        Ok(())
    }

    fn withdraw(&mut self, transaction_id: u32, amount: Amount) -> Result<()> {
        match self.transaction_log.entry(transaction_id) {
            Entry::Occupied(_) => Err(eyre!(
                "we have already processed transaction id {}",
                transaction_id
            )),
            Entry::Vacant(v) => {
                if self.available >= amount {
                    self.available -= amount;
                    v.insert(None);
                    Ok(())
                } else {
                    Err(eyre!(
                        "unable to withdraw as the account does not have enough available funds"
                    ))
                }
            }
        }
    }

    fn dispute(&mut self, transaction_id: u32, amount: Amount) -> Result<()> {
        self.available -= amount;
        self.held += amount;

        self.transaction_log
            .insert(transaction_id, Some(Transaction::Dispute { amount }));
        Ok(())
    }

    fn resolve(&mut self, transaction_id: u32, amount: Amount) -> Result<()> {
        self.held -= amount;
        self.available += amount;

        // This is an optimization based off the **Valid State Transitions** assumption
        // in the readme.
        //
        // If we enter this state, this transaction id can no longer be modified, therefore we can
        // completely remove the associated data.
        self.transaction_log.insert(transaction_id, None);
        Ok(())
    }

    fn chargeback(&mut self, amount: Amount) -> Result<()> {
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
        Ok(())
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
        let client = Client {
            id: 1,
            available: Amount::new(22.2f32)?,
            held: Amount::new(3.32f32)?,
            status: AccountStatus::Active,
            transaction_log: Default::default(),
        };
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
        let mut client = Client {
            id: 1,
            available: Amount::new(20f32)?,
            held: Amount::default(),
            status: AccountStatus::Frozen,
            transaction_log: Default::default(),
        };
        let tx_id = 1;
        for tx in &[
            TransactionType::Deposit,
            TransactionType::Withdrawal,
            TransactionType::Dispute,
            TransactionType::Resolve,
            TransactionType::Chargeback,
        ] {
            let res = client.publish_transaction(tx_id, *tx, None);
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
        client.publish_transaction(tx_id, TransactionType::Deposit, Some(Amount::new(tx_amt)?))?;
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
        let mut client = Client::new(1);
        let tx_id = 1;
        let tx_amt = 1.23f32;
        client.publish_transaction(tx_id, TransactionType::Deposit, Some(Amount::new(tx_amt)?))?;
        assert_eq!(
            client.transaction_log.len(),
            1,
            "transaction log has a size of 1"
        );
        let prev_available_funds = client.available_funds()?;
        assert_eq!(prev_available_funds, tx_amt);

        // duplicate
        let result =
            client.publish_transaction(tx_id, TransactionType::Deposit, Some(Amount::new(tx_amt)?));
        assert!(result.is_err(), "duplicate transaction should error");

        let log_data = client.transaction_log.get(&tx_id);
        assert!(
            log_data.is_some(),
            "the existing transaction should still be there"
        );
        assert!(
            log_data.unwrap().is_some(),
            "the existing nested transaction id should still be there"
        );
        let is_deposit_type = matches!(log_data.unwrap().unwrap(), Transaction::Deposit { .. });
        assert!(
            is_deposit_type,
            "the existing deposit type should still be there"
        );
        assert_eq!(
            prev_available_funds,
            client.available_funds()?,
            "available funds shouldn't increase"
        );

        Ok(())
    }

    #[test]
    fn handles_a_withdrawal_with_a_new_transaction_id() -> Result<()> {
        let mut client = Client {
            id: 1,
            available: Amount::new(20f32)?,
            held: Amount::default(),
            status: AccountStatus::Active,
            transaction_log: Default::default(),
        };
        let tx_id = 1;
        let tx_amt = 1.23f32;
        client.publish_transaction(
            tx_id,
            TransactionType::Withdrawal,
            Some(Amount::new(tx_amt)?),
        )?;
        assert_eq!(
            client.transaction_log.len(),
            1,
            "transaction log has a size of 1"
        );
        assert_eq!(client.available_funds()?, 20f32 - tx_amt);
        assert_eq!(client.held_funds()?, 0f32);
        let log_data = client.transaction_log.get(&tx_id);
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
        let mut client = Client {
            id: 1,
            available: Amount::new(20f32)?,
            held: Amount::default(),
            status: AccountStatus::Active,
            transaction_log: Default::default(),
        };
        let tx_id = 1;
        let tx_amt = 1.23f32;
        client.publish_transaction(
            tx_id,
            TransactionType::Withdrawal,
            Some(Amount::new(tx_amt)?),
        )?;
        assert_eq!(
            client.transaction_log.len(),
            1,
            "transaction log has a size of 1"
        );
        let expected_available_funds = 20f32 - tx_amt;
        let prev_available_funds = client.available_funds()?;
        assert_eq!(prev_available_funds, expected_available_funds);

        // duplicate
        let result = client.publish_transaction(
            tx_id,
            TransactionType::Withdrawal,
            Some(Amount::new(tx_amt)?),
        );
        assert!(result.is_err(), "duplicate transaction should error");

        let log_data = client.transaction_log.get(&tx_id);
        assert!(
            log_data.is_some(),
            "the existing transaction should still be there"
        );
        assert!(
            log_data.unwrap().is_none(),
            "we don't need to store any data for withdrawals other than the transaction id"
        );
        assert_eq!(
            prev_available_funds,
            client.available_funds()?,
            "available funds shouldn't decrease"
        );

        Ok(())
    }

    #[test]
    fn handles_a_dispute_on_a_deposit() -> Result<()> {
        let mut client = Client::new(1);
        let tx_id = 1;
        let tx_amt = 1.23f32;
        client.publish_transaction(tx_id, TransactionType::Deposit, Some(Amount::new(tx_amt)?))?;
        assert_eq!(
            client.transaction_log.len(),
            1,
            "transaction log has a size of 1"
        );
        let previous_available_balance = client.available_funds()?;
        client.publish_transaction(tx_id, TransactionType::Dispute, None)?;
        assert_eq!(
            client.available_funds()?,
            0f32,
            "available funds should be reduced to zero"
        );
        assert_eq!(
            client.held_funds()?,
            previous_available_balance,
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
        let mut client = Client::new(1);
        let tx_id = 1;
        let tx_amt = 1.23f32;
        client.publish_transaction(tx_id, TransactionType::Deposit, Some(Amount::new(tx_amt)?))?;
        assert_eq!(
            client.transaction_log.len(),
            1,
            "transaction log has a size of 1"
        );
        let previous_available_balance = client.available_funds()?;
        client.publish_transaction(tx_id, TransactionType::Dispute, None)?;
        let res = client.publish_transaction(tx_id, TransactionType::Dispute, None);
        assert!(res.is_err());
        assert_eq!(
            client.available_funds()?,
            0f32,
            "available funds should be reduced to zero"
        );
        assert_eq!(
            client.held_funds()?,
            previous_available_balance,
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
    fn handles_a_resolve_on_a_dispute() -> Result<()> {
        let mut client = Client::new(1);
        let tx_id = 1;
        let tx_amt = 1.23f32;
        client.publish_transaction(tx_id, TransactionType::Deposit, Some(Amount::new(tx_amt)?))?;
        assert_eq!(
            client.transaction_log.len(),
            1,
            "transaction log has a size of 1"
        );
        client.publish_transaction(tx_id, TransactionType::Dispute, None)?;
        client.publish_transaction(tx_id, TransactionType::Resolve, None)?;
        assert_eq!(
            client.available_funds()?,
            tx_amt,
            "available funds should be back to the full amount"
        );
        assert_eq!(client.held_funds()?, 0f32, "held funds should be back to 0");

        let log_data = client.transaction_log.get(&tx_id);
        assert!(
            log_data.is_some(),
            "the transaction id should be present in the log"
        );
        assert!(
            log_data.unwrap().is_none(),
            "we no longer need to hold transaction data"
        );
        Ok(())
    }

    #[test]
    fn should_ignore_duplicate_resolve_requests() -> Result<()> {
        let mut client = Client::new(1);
        let tx_id = 1;
        let tx_amt = 1.23f32;
        client.publish_transaction(tx_id, TransactionType::Deposit, Some(Amount::new(tx_amt)?))?;
        assert_eq!(
            client.transaction_log.len(),
            1,
            "transaction log has a size of 1"
        );
        client.publish_transaction(tx_id, TransactionType::Dispute, None)?;
        client.publish_transaction(tx_id, TransactionType::Resolve, None)?;
        let res = client.publish_transaction(tx_id, TransactionType::Resolve, None);
        assert!(res.is_err());
        assert_eq!(
            client.available_funds()?,
            tx_amt,
            "available funds should be back to the full amount"
        );
        assert_eq!(client.held_funds()?, 0f32, "held funds should be back to 0");

        let log_data = client.transaction_log.get(&tx_id);
        assert!(
            log_data.is_some(),
            "the transaction id should be present in the log"
        );
        assert!(
            log_data.unwrap().is_none(),
            "we no longer need to hold transaction data"
        );
        Ok(())
    }

    #[test]
    fn handles_a_chargeback_on_a_dispute() -> Result<()> {
        let mut client = Client::new(1);
        let tx_id = 1;
        let tx_amt = 1.23f32;
        client.publish_transaction(tx_id, TransactionType::Deposit, Some(Amount::new(tx_amt)?))?;
        assert_eq!(
            client.transaction_log.len(),
            1,
            "transaction log has a size of 1"
        );
        client.publish_transaction(tx_id, TransactionType::Dispute, None)?;
        client.publish_transaction(tx_id, TransactionType::Chargeback, None)?;
        assert_eq!(
            client.available_funds()?,
            0f32,
            "available funds should be 0 if a chargeback occurs"
        );
        assert_eq!(
            client.held_funds()?,
            0f32,
            "held funds should be 0 if a chargeback occurs"
        );
        assert_eq!(
            client.status,
            AccountStatus::Frozen,
            "the account should be frozen"
        );

        assert_eq!(
            client.transaction_log.len(),
            0,
            "the transaction log should have been emptied"
        );
        Ok(())
    }

    #[test]
    fn handles_illegal_transitions_from_deposit() -> Result<()> {
        let mut client = Client::new(1);
        let tx_id = 1;
        let tx_amt = 1.23f32;
        client.publish_transaction(tx_id, TransactionType::Deposit, Some(Amount::new(tx_amt)?))?;

        for transition in &[
            TransactionType::Withdrawal,
            TransactionType::Resolve,
            TransactionType::Chargeback,
        ] {
            let mut cli = client.clone();
            let prev_funds = cli.available_funds()?;
            let prev_total_funds = cli.total_funds()?;
            let result = cli.publish_transaction(tx_id, *transition, Some(Amount::new(tx_amt)?));
            assert!(result.is_err());
            assert_eq!(prev_funds, cli.available_funds()?);
            assert_eq!(prev_total_funds, cli.total_funds()?);
        }
        Ok(())
    }

    #[test]
    fn handles_illegal_transitions_from_withdrawal() -> Result<()> {
        let mut client = Client {
            id: 1,
            available: Amount::new(20f32)?,
            held: Amount::default(),
            status: AccountStatus::Active,
            transaction_log: Default::default(),
        };
        let tx_id = 1;
        let tx_amt = 1.23f32;
        client.publish_transaction(
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
            let mut cli = client.clone();
            let prev_funds = cli.available_funds()?;
            let prev_total_funds = cli.total_funds()?;
            let result = cli.publish_transaction(tx_id, *transition, Some(Amount::new(tx_amt)?));
            assert!(result.is_err());
            assert_eq!(prev_funds, cli.available_funds()?);
            assert_eq!(prev_total_funds, cli.total_funds()?);
        }
        Ok(())
    }

    #[test]
    fn handles_illegal_transitions_from_dispute() -> Result<()> {
        let mut client = Client::new(1);
        let tx_id = 1;
        let tx_amt = 1.23f32;
        client.publish_transaction(tx_id, TransactionType::Deposit, Some(Amount::new(tx_amt)?))?;
        client.publish_transaction(tx_id, TransactionType::Dispute, None)?;

        for transition in &[TransactionType::Deposit, TransactionType::Withdrawal] {
            let mut cli = client.clone();
            let prev_funds = cli.available_funds()?;
            let prev_total_funds = cli.total_funds()?;
            let result = cli.publish_transaction(tx_id, *transition, Some(Amount::new(tx_amt)?));
            assert!(result.is_err());
            assert_eq!(prev_funds, cli.available_funds()?);
            assert_eq!(prev_total_funds, cli.total_funds()?);
        }
        Ok(())
    }

    #[test]
    fn handles_illegal_transitions_from_resolve() -> Result<()> {
        let mut client = Client::new(1);
        let tx_id = 1;
        let tx_amt = 1.23f32;
        client.publish_transaction(tx_id, TransactionType::Deposit, Some(Amount::new(tx_amt)?))?;
        client.publish_transaction(tx_id, TransactionType::Dispute, None)?;
        client.publish_transaction(tx_id, TransactionType::Resolve, None)?;

        for transition in &[
            TransactionType::Deposit,
            TransactionType::Withdrawal,
            TransactionType::Dispute,
            TransactionType::Chargeback,
        ] {
            let mut cli = client.clone();
            let prev_funds = cli.available_funds()?;
            let prev_total_funds = cli.total_funds()?;
            let result = cli.publish_transaction(tx_id, *transition, Some(Amount::new(tx_amt)?));
            assert!(result.is_err());
            assert_eq!(prev_funds, cli.available_funds()?);
            assert_eq!(prev_total_funds, cli.total_funds()?);
        }
        Ok(())
    }
}
