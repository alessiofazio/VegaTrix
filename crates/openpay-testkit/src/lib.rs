use openpay_domain::{AmountMinor, Currency, MerchantId, PaymentId, TenantId};

pub fn demo_amount() -> AmountMinor {
    AmountMinor::new(1200).expect("demo amount")
}

pub fn demo_currency() -> Currency {
    Currency::EUR
}

pub fn prefixed_ids() -> (TenantId, MerchantId, PaymentId) {
    (TenantId::new(), MerchantId::new(), PaymentId::new())
}
