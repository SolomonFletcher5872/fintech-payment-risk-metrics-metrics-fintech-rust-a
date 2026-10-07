use crate::infrai_client::{InfraiClient, InfraiError};
use serde::Serialize;
use serde_json::{json, Value};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentOutcome {
    Authorized,
    Declined,
}

#[derive(Debug, Clone)]
pub struct PaymentEvent {
    pub event_id: String,
    pub payment_id: String,
    pub amount_minor: u64,
    pub currency: String,
    pub risk_score: u8,
    pub outcome: PaymentOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskAction {
    Settle,
    ManualReview,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditNotification {
    pub event_id: String,
    pub payment_id: String,
    pub action: RiskAction,
    pub reason: &'static str,
}

#[derive(Debug, Serialize)]
pub struct OperationsChart {
    pub payment_metric: String,
    pub payment_value: f64,
    pub platform_usage: Value,
}

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("invalid payment event: {0}")]
    InvalidEvent(&'static str),
    #[error(transparent)]
    Infrai(#[from] InfraiError),
}

impl ServiceError {
    pub fn caller_status(&self) -> u16 {
        match self {
            Self::InvalidEvent(_) => 400,
            Self::Infrai(error) => error.caller_status(),
        }
    }
}

pub fn decide(event: &PaymentEvent) -> Result<AuditNotification, ServiceError> {
    if event.event_id.trim().is_empty() || event.payment_id.trim().is_empty() {
        return Err(ServiceError::InvalidEvent("event and payment ids are required"));
    }
    if event.amount_minor == 0 {
        return Err(ServiceError::InvalidEvent("amount must be positive"));
    }
    if event.currency.len() != 3 {
        return Err(ServiceError::InvalidEvent("currency must be a three-letter code"));
    }
    if event.risk_score > 100 {
        return Err(ServiceError::InvalidEvent("risk score must be between 0 and 100"));
    }

    let (action, reason) = match (event.outcome, event.risk_score) {
        (PaymentOutcome::Declined, _) => (RiskAction::Reject, "processor_declined"),
        (PaymentOutcome::Authorized, 80..=100) => {
            (RiskAction::ManualReview, "risk_score_at_or_above_80")
        }
        (PaymentOutcome::Authorized, _) => (RiskAction::Settle, "authorized_below_review_threshold"),
    };
    Ok(AuditNotification {
        event_id: event.event_id.clone(),
        payment_id: event.payment_id.clone(),
        action,
        reason,
    })
}

pub async fn process_payment(
    client: &InfraiClient,
    event: &PaymentEvent,
) -> Result<(AuditNotification, OperationsChart), ServiceError> {
    let notification = decide(event)?;
    let metric_name = match notification.action {
        RiskAction::Settle => "payments.settle",
        RiskAction::ManualReview => "payments.manual_review",
        RiskAction::Reject => "payments.reject",
    };
    let tags = json!({
        "currency": event.currency,
        "risk_action": notification.action,
    });
    client
        .report_metric("counter", metric_name, 1.0, tags, &event.event_id)
        .await?;
    client
        .report_metric(
            "gauge",
            "payments.risk_score",
            f64::from(event.risk_score),
            json!({ "currency": event.currency }),
            &format!("{}-risk", event.event_id),
        )
        .await?;
    let platform_usage = client.usage_timeseries().await?;

    Ok((
        notification,
        OperationsChart {
            payment_metric: metric_name.to_owned(),
            payment_value: 1.0,
            platform_usage,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorized_high_risk_payment_is_held_for_review() {
        let event = PaymentEvent {
            event_id: "evt-test-91".into(),
            payment_id: "pay-774".into(),
            amount_minor: 25_000,
            currency: "USD".into(),
            risk_score: 91,
            outcome: PaymentOutcome::Authorized,
        };

        let notice = decide(&event).expect("valid payment event");

        assert_eq!(notice.action, RiskAction::ManualReview);
        assert_eq!(notice.reason, "risk_score_at_or_above_80");
        assert_eq!(notice.event_id, "evt-test-91");
    }

    #[test]
    fn processor_decline_is_rejected_even_with_low_risk() {
        let event = PaymentEvent {
            event_id: "evt-test-12".into(),
            payment_id: "pay-775".into(),
            amount_minor: 1_250,
            currency: "EUR".into(),
            risk_score: 12,
            outcome: PaymentOutcome::Declined,
        };

        assert_eq!(decide(&event).unwrap().action, RiskAction::Reject);
    }
}

