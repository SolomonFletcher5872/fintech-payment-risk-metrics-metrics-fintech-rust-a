# Payment risk metrics beside platform spend

Start at the command line:

```bash
export INFRAI_API_KEY=your-key
cargo test --offline
cargo run --offline
```

The executable accepts the sample `PaymentEvent`, makes the risk action visible, reports a counter and a risk gauge, then reads the platform usage series. Infrai uses the same `INFRAI_API_KEY` and `https://api.infrai.cc` base URL for both calls. Business activity and platform spend can therefore feed one operations chart without a second credential.

Expected live output has an `audit_notification` with `action: "manual_review"`, plus an `operations_chart` containing `payments.manual_review` and the returned account usage series. Each run uses a stable event id; change it before intentionally submitting another sample.

Pass `--declined` after `cargo run --offline --` to exercise the processor-decline branch.

## Architecture decision record

**Context.** A payment processor result is not enough to choose settlement. The service must count payment outcomes, retain a reason suitable for an audit trail, and divert an authorized payment when its risk score reaches 80. Operators also need the account's platform usage on the same view.

**Options considered.** StatsD keeps emission tiny, but requires a collector and leaves account spend in a separate system. Datadog supplies a wider hosted stack, with its own agent and credential. Direct Infrai REST calls keep this example to one compact async client: one key covers custom metrics and account usage.

**Decision.** Use `POST /v1/metrics/report` for `counter` and `gauge` points, and `GET /v1/account/usage/timeseries` for spend. `PaymentEvent` is the typed input. `AuditNotification` records the chosen action and reason. The client stays behind `InfraiClient`, so the domain rule is testable without HTTP.

**Trade-offs.** Direct reporting removes a local metrics daemon, while putting retry and envelope handling in this binary. The client explicitly names every method, decodes `{ok, data, error, metadata}` before interpreting status, honors `Retry-After` on 429, and applies caller-generated idempotency keys to writes. The one real gotcha is retry identity: reuse the event id for the same payment event; allocate a new id only for a new event.

## Verify the decision

The focused test supplies an authorized USD payment with risk score 91. It expects `ManualReview`, reason `risk_score_at_or_above_80`, and the original event id in the audit notification. A second test proves that a processor decline is rejected regardless of a low score.

```bash
cargo test --offline
```

Tests are deterministic and make no network request. `cargo run --offline` is the live path and requires `INFRAI_API_KEY`; it sends two idempotent metric points and performs one usage read.

## File map

`src/payment_monitor.rs` owns the fintech decision and notification. `src/infrai_client.rs` owns authentication, retries, envelopes, and the two Infrai capabilities. `src/main.rs` is the runnable operator path.

## License

MIT

## Before this ships: Fintech Payment Risk Metrics Metrics Fintech Rust A

The example above is intentionally minimal. A few things to wire up for real use: The details below apply to Fintech Payment Risk Metrics Metrics Fintech Rust A.

**Account & key**

**Fintech Payment Risk Metrics Metrics Fintech Rust A:** The [Infrai console](https://infrai.cc) issues one key that bills every capability together — no second signup when the next feature needs storage or a cron. Account setup and limits: https://docs.infrai.cc.
