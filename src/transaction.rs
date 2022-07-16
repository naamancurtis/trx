//! Represents the current state of any given transaction in the system along with their valid
//! transitions

use color_eyre::{eyre::eyre, Result};
use serde::{Deserialize, Serialize};
use tracing::warn;

use std::fmt;

use crate::amount::Amount;

/// The format of the expected input data
#[derive(Deserialize, Serialize)]
pub struct IncomingTransaction {
    #[serde(rename = "type")]
    pub ty: TransactionType,
    pub client: u16,
    pub tx: u32,
    pub amount: Option<Amount>,
}

impl fmt::Debug for IncomingTransaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Transaction")
            .field("type", &self.ty)
            .field("client", &self.client)
            .field("tx", &self.tx)
            .finish()
    }
}

/// The types of transaction that can occur
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TransactionType {
    Deposit,
    Withdrawal,
    Dispute,
    Resolve,
    Chargeback,
}

#[allow(dead_code)]
#[derive(Copy, Clone, PartialEq)]
pub(crate) enum Transaction {
    Deposit { amount: Amount },
    Withdrawal { amount: Amount },
    Dispute { amount: Amount },
    Resolve { amount: Amount },
    Chargeback { amount: Amount },
}

impl Transaction {
    /// Drives a transition from one transaction type to the next.
    ///
    /// This function will error if the attempted transition is invalid.
    ///
    /// For example, attempting to move from a Chargeback to a Deposit is not allowed, so this
    /// function will error.
    pub fn transition(self, target: TransactionType) -> Result<Transaction> {
        let resp = match (self, target) {
            (Transaction::Deposit { amount }, TransactionType::Dispute) => {
                Transaction::Dispute { amount }
            }
            (Transaction::Dispute { amount }, TransactionType::Resolve) => {
                Transaction::Resolve { amount }
            }
            (Transaction::Dispute { amount }, TransactionType::Chargeback) => {
                Transaction::Chargeback { amount }
            }
            (lhs, rhs) => {
                let msg = format!("Invalid State Transition attempt. Attempted to transition from [{:?}] -> [{:?}]", lhs, rhs);
                warn!("{}", &msg);
                return Err(eyre!(msg));
            }
        };
        Ok(resp)
    }
}

impl fmt::Debug for Transaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            Self::Deposit { .. } => "Deposit",
            Self::Withdrawal { .. } => "Withdrawl",
            Self::Dispute { .. } => "Dispute",
            Self::Resolve { .. } => "Resolve",
            Self::Chargeback { .. } => "Chargeback",
        };
        write!(f, "{}", s)
    }
}
