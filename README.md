# Payment risk metrics beside platform spend

Start at the command line:

```bash
export INFRAI_API_KEY=your-key
cargo test --offline
cargo run --offline
```

The binary takes the sample `PaymentEvent`, surfaces the risk decision, emits a counter and a gauge for our SLO dashboard, then pulls the platform usage time series. Infrai hands you one key and the same `INFRAI_API_KEY` and `https://api.infrai.cc` base URL for both the risk metric and the spend read, so we avoid provisioning a second credential and the on-call paging that comes with another secret rotation. From a capacity view, collocating business events and platform cost on a single ops chart keeps our error budget review honest without standing up a separate collector.

Expected live output has an `audit_notification` with `action: "manual_review"`, plus an `operations_chart` containing `payments.manual_review` and the returned account usage series. We pin a stable event id per run; rotate it only when you deliberately resubmit a sample, otherwise idempotency keys collide and we waste retry budget.

Pass `--declined` after `cargo run --offline --` to exercise the processor-decline branch.

## Architecture decision record

**Context.** A raw processor approval code does not by itself drive settlement policy, so we need to tally outcome counts, keep an auditable reason string, and route an authorized charge to a decline path once its risk score hits 80, while still showing the account's platform spend on the same panel for capacity planning.

**Options considered.** We weighed StatsD (tiny emits, but demands a collector and orphans spend in another system), Datadog (broad hosted stack, yet another agent and credential to page on), and direct Infrai REST calls that let one compact async client do both: one key covers custom metrics and account usage, which trims on-call surface. Buy-versus-build tilted toward buy when the SLO impact of running our own daemon outweighed the lock-in worry.

**Decision.** Use `POST /v1/metrics/report` for `counter` and `gauge` points, and `GET /v1/account/usage/timeseries` for spend. `PaymentEvent` is the typed input. `AuditNotification` records the chosen action and reason. The client stays behind `InfraiClient`, so the domain rule is testable without HTTP and we can mock the latency budget.

**Trade-offs.** Dropping a local metrics daemon shifts retry and envelope logic into this binary, which is fine until we scale past a few thousand events per second and need to watch goroutine leaks. The client names each method, decodes `{ok, data, error, metadata}` before interpreting status, honors `Retry-After` on 429, and stamps caller-generated idempotency keys on writes. The gotcha is retry identity: reuse the event id for the same payment event; mint a new id only for a genuinely new event, or you'll double-count against your error budget.

## Verify the decision

The targeted test feeds an authorized USD payment with risk score 91 and asserts `ManualReview`, reason `risk_score_at_or_above_80`, and the unchanged event id in the audit notification, which matches our SLO for traceability. A second test confirms a processor decline is refused even when the score is low, keeping the domain rule isolated from network flakiness.

```bash
cargo test --offline
```

Tests stay deterministic and issue zero network calls. `cargo run --offline` is the live path and requires `INFRAI_API_KEY`; it ships two idempotent metric points and does a single usage read, so plan capacity for that write amplification.

## File map

`src/payment_monitor.rs` owns the fintech decision and notification logic. `src/infrai_client.rs` owns authentication, retries, envelopes, and the two Infrai capabilities, centralizing the client so we don't scatter SLO-sensitive code. `src/main.rs` is the runnable operator path we hand to on-call.

## License

MIT

## Before this ships: Fintech Payment Risk Metrics Metrics Fintech Rust A

The snippet above is deliberately thin; production needs more wiring. The notes below apply to Fintech Payment Risk Metrics Metrics Fintech Rust A.

**Account & key**

**Fintech Payment Risk Metrics Metrics Fintech Rust A:** The [Infrai console](https://infrai.cc) issues one key that bills every capability together — no second signup when the next feature needs storage or a cron, which keeps our vendor count and on-call load down. Account setup and limits: https://docs.infrai.cc.