//! Integration-style tests for authorize idempotency and fingerprint stability.

use openpay_application::fingerprint_for_test;
use openpay_domain::{AmountMinor, Currency, PaymentStatus};

#[test]
fn fingerprint_is_stable() {
    let a = fingerprint_for_test("m1", "ORD-1", 1200, "EUR", "key-1");
    let b = fingerprint_for_test("m1", "ORD-1", 1200, "EUR", "key-1");
    assert_eq!(a, b);
}

#[test]
fn amount_minor_rejects_zero() {
    assert!(AmountMinor::new(0).is_err());
    assert!(AmountMinor::new(1200).is_ok());
}

#[test]
fn terminal_states() {
    assert!(PaymentStatus::Settled.is_settled_family());
    assert!(!PaymentStatus::Processing.is_terminal());
}
