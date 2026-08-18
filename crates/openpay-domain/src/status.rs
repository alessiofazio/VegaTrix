use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

use crate::error::DomainError;

/// Canonical OpenPay payment statuses. Adapter statuses must map here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PaymentStatus {
    Created,
    Pending,
    RequiresAction,
    Authorized,
    Processing,
    Settled,
    Failed,
    Cancelled,
    Expired,
    RefundPending,
    Refunded,
    PartiallyRefunded,
}

impl Display for PaymentStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PaymentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "CREATED",
            Self::Pending => "PENDING",
            Self::RequiresAction => "REQUIRES_ACTION",
            Self::Authorized => "AUTHORIZED",
            Self::Processing => "PROCESSING",
            Self::Settled => "SETTLED",
            Self::Failed => "FAILED",
            Self::Cancelled => "CANCELLED",
            Self::Expired => "EXPIRED",
            Self::RefundPending => "REFUND_PENDING",
            Self::Refunded => "REFUNDED",
            Self::PartiallyRefunded => "PARTIALLY_REFUNDED",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Failed
                | Self::Cancelled
                | Self::Expired
                | Self::Refunded
        )
    }

    pub fn is_settled_family(self) -> bool {
        matches!(
            self,
            Self::Settled | Self::RefundPending | Self::Refunded | Self::PartiallyRefunded
        )
    }

    pub fn allowed_targets(self) -> &'static [PaymentStatus] {
        use PaymentStatus::*;
        match self {
            Created => &[Pending],
            Pending => &[RequiresAction, Processing, Failed, Cancelled, Expired],
            RequiresAction => &[Processing, Failed, Cancelled, Expired],
            Processing => &[Authorized, Settled, Failed],
            Authorized => &[Settled, Failed, Cancelled],
            Settled => &[RefundPending, PartiallyRefunded, Refunded],
            RefundPending => &[Refunded, PartiallyRefunded, Failed],
            PartiallyRefunded => &[Refunded, RefundPending],
            Failed | Cancelled | Expired | Refunded => &[],
        }
    }

    pub fn can_transition_to(self, next: PaymentStatus) -> bool {
        self.allowed_targets().contains(&next)
    }

    pub fn transition(self, next: PaymentStatus) -> Result<PaymentStatus, DomainError> {
        if self == next {
            return Ok(self);
        }
        if self.can_transition_to(next) {
            Ok(next)
        } else {
            Err(DomainError::IllegalTransition {
                from: self.as_str().to_string(),
                to: next.as_str().to_string(),
            })
        }
    }

    pub fn webhook_event(self) -> Option<&'static str> {
        Some(match self {
            Self::Created => "payment.created",
            Self::RequiresAction => "payment.requires_action",
            Self::Processing | Self::Pending => "payment.processing",
            Self::Authorized => "payment.authorized",
            Self::Settled => "payment.settled",
            Self::Failed => "payment.failed",
            Self::Cancelled => "payment.cancelled",
            Self::Expired => "payment.expired",
            Self::Refunded | Self::PartiallyRefunded => "payment.refunded",
            Self::RefundPending => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AttemptStatus {
    Created,
    RequiresAction,
    Processing,
    Authorized,
    Settled,
    Failed,
    Cancelled,
    Expired,
    Ambiguous,
}

impl AttemptStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "CREATED",
            Self::RequiresAction => "REQUIRES_ACTION",
            Self::Processing => "PROCESSING",
            Self::Authorized => "AUTHORIZED",
            Self::Settled => "SETTLED",
            Self::Failed => "FAILED",
            Self::Cancelled => "CANCELLED",
            Self::Expired => "EXPIRED",
            Self::Ambiguous => "AMBIGUOUS",
        }
    }

    pub fn into_payment_status(self) -> PaymentStatus {
        match self {
            Self::Created => PaymentStatus::Pending,
            Self::RequiresAction => PaymentStatus::RequiresAction,
            Self::Processing => PaymentStatus::Processing,
            Self::Authorized => PaymentStatus::Authorized,
            Self::Settled => PaymentStatus::Settled,
            Self::Failed => PaymentStatus::Failed,
            Self::Cancelled => PaymentStatus::Cancelled,
            Self::Expired => PaymentStatus::Expired,
            Self::Ambiguous => PaymentStatus::Processing,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PaymentMethod {
    AccountToAccount,
    Card,
    Wallet,
    Manual,
}

impl PaymentMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AccountToAccount => "ACCOUNT_TO_ACCOUNT",
            Self::Card => "CARD",
            Self::Wallet => "WALLET",
            Self::Manual => "MANUAL",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn created_to_pending_ok() {
        assert!(PaymentStatus::Created.transition(PaymentStatus::Pending).is_ok());
    }

    #[test]
    fn settled_to_pending_rejected() {
        assert!(PaymentStatus::Settled.transition(PaymentStatus::Pending).is_err());
    }

    #[test]
    fn ambiguous_maps_to_processing() {
        assert_eq!(
            AttemptStatus::Ambiguous.into_payment_status(),
            PaymentStatus::Processing
        );
    }

    #[test]
    fn same_status_is_idempotent() {
        assert_eq!(
            PaymentStatus::Settled.transition(PaymentStatus::Settled).unwrap(),
            PaymentStatus::Settled
        );
    }
}
