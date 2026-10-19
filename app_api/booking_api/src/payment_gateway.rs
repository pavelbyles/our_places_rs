//! Payment settlement gateway abstraction and simulated implementation for reservation changes.

use common::models::ChangeSettlementStatus;
use rust_decimal::Decimal;
use std::future::Future;
use std::pin::Pin;
use uuid::Uuid;

/// Boxed future type returned by [`PaymentSettlementGateway::settle_balance_delta`].
pub type SettlementFuture<'a> = Pin<
    Box<dyn Future<Output = Result<(ChangeSettlementStatus, Option<String>), String>> + Send + 'a>,
>;

/// Abstraction for charging or refunding balance differences on reservation changes.
pub trait PaymentSettlementGateway: Send + Sync {
    /// Settles a positive (`charge`) or negative (`refund`) balance for a booking change request.
    /// Returns `(ChangeSettlementStatus, Option<String>)` containing the resulting status and gateway reference.
    fn settle_balance_delta<'a>(
        &'a self,
        booking_id: Uuid,
        change_request_id: Uuid,
        currency: &'a str,
        balance_due: Decimal,
    ) -> SettlementFuture<'a>;
}

/// Default simulated payment gateway used before external card processor integration.
#[derive(Debug, Default, Clone)]
pub struct SimulatedPaymentGateway;

impl PaymentSettlementGateway for SimulatedPaymentGateway {
    fn settle_balance_delta<'a>(
        &'a self,
        booking_id: Uuid,
        change_request_id: Uuid,
        currency: &'a str,
        balance_due: Decimal,
    ) -> SettlementFuture<'a> {
        Box::pin(async move {
            if balance_due.is_zero() {
                return Ok((ChangeSettlementStatus::NotRequired, None));
            }

            let reference = if balance_due > Decimal::ZERO {
                tracing::info!(
                    %booking_id,
                    %change_request_id,
                    %currency,
                    %balance_due,
                    "Simulated payment gateway charged reservation change balance"
                );
                (
                    ChangeSettlementStatus::Charged,
                    Some(format!("SIM-CHG-{}", Uuid::now_v7().simple())),
                )
            } else {
                tracing::info!(
                    %booking_id,
                    %change_request_id,
                    %currency,
                    %balance_due,
                    "Simulated payment gateway refunded reservation change balance"
                );
                (
                    ChangeSettlementStatus::Refunded,
                    Some(format!("SIM-REF-{}", Uuid::now_v7().simple())),
                )
            };

            Ok(reference)
        })
    }
}
