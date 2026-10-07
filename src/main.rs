mod infrai_client;
mod payment_monitor;

use infrai_client::InfraiClient;
use payment_monitor::{process_payment, PaymentEvent, PaymentOutcome};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("payment metrics service: {error}");
        std::process::exit(if error.caller_status() < 500 { 2 } else { 1 });
    }
}

async fn run() -> Result<(), payment_monitor::ServiceError> {
    let client = InfraiClient::from_env()?;
    let outcome = if std::env::args().any(|arg| arg == "--declined") {
        PaymentOutcome::Declined
    } else {
        PaymentOutcome::Authorized
    };
    let event = PaymentEvent {
        event_id: "evt-demo-20260928".into(),
        payment_id: "pay-1042".into(),
        amount_minor: 42_50,
        currency: "USD".into(),
        risk_score: 86,
        outcome,
    };
    let (notification, chart) = process_payment(&client, &event).await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "audit_notification": notification,
            "operations_chart": chart,
        }))
        .expect("serializable output")
    );
    Ok(())
}
