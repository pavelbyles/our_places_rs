#!/usr/bin/env python3
"""
Skill Benchmark Evaluation Runner
Evaluates agent completions against synthetic benchmark assertions defined in skills_benchmarks.json.
"""

import json
import re
import sys
from pathlib import Path

BENCHMARK_FILE = Path(__file__).parent / "skills_benchmarks.json"

def load_benchmarks():
    if not BENCHMARK_FILE.exists():
        print(f"Error: Benchmark file not found at {BENCHMARK_FILE}")
        sys.exit(1)
    with open(BENCHMARK_FILE, "r", encoding="utf-8") as f:
        return json.load(f)

def run_assertions(benchmark, completion_text):
    results = []
    for assertion in benchmark.get("assertions", []):
        atype = assertion.get("type")
        val = assertion.get("value", "")
        reason = assertion.get("reason", "")
        
        passed = False
        if atype == "contains":
            passed = val in completion_text
        elif atype == "not_contains":
            passed = val not in completion_text
        elif atype == "contains_pattern":
            passed = bool(re.search(assertion.get("pattern", ""), completion_text, re.MULTILINE))
        
        results.append({
            "assertion": assertion,
            "passed": passed,
            "reason": reason
        })
    return results

def get_exemplars():
    return {
        "rust-core:Zero-Unwrap & Error Propagation Test": """
use actix_web::{web, HttpResponse};
use common::models::AppError;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct PaymentRequest {
    pub user_id: Uuid,
    pub amount: String,
}

pub async fn handle_payment(
    payload: web::Json<PaymentRequest>,
) -> Result<HttpResponse, AppError> {
    let amount = Decimal::from_str(&payload.amount)
        .map_err(|e| AppError::ValidationError(format!("Invalid decimal: {e}")))?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "user_id": payload.user_id, "amount": amount })))
}
""",
        "rust-core:Async Non-Blocking I/O Test": """
use common::models::AppError;
use tokio::task::spawn_blocking;

pub async fn hash_password(password: String) -> Result<String, AppError> {
    spawn_blocking(move || {
        bcrypt::hash(password, bcrypt::DEFAULT_COST)
            .map_err(|e| AppError::InternalError(format!("Hash failed: {e}")))
    })
    .await
    .map_err(|e| AppError::InternalError(format!("Task failed: {e}")))?
}
""",
        "monad-design:Railway-Oriented Combinator Pipeline Test": """
pub fn process_booking_payment(
    req: BookingRequest,
    repo: &BookingRepository,
) -> Result<BookingHold, AppError> {
    validate_date_range(&req.dates)
        .and_then(|dates| repo.check_availability(req.villa_id, &dates))
        .and_then(|avail| apply_discount_code(req.amount, req.discount_code))
        .and_then(|final_amount| repo.commit_hold(req.villa_id, req.dates, final_amount))
}
""",
        "grill-me:Decision-Tree Stress Test": """
Before implementing multi-currency payouts to hosts in Jamaica, I need to clarify a few key assumptions and requirements:

1. **FX Rate Sourcing & Locking**:
   - (Recommended) Lock the USD/JMD rate for 15 minutes during the checkout/payout window via the official exchange rate cache.
   - Settle at real-time market spot rate upon bank transfer execution.
   *Which exchange rate model should we adhere to?*

2. **Statutory GCT & Tax Compliance**:
   - (Recommended) Deduct 15% statutory Jamaican GCT strictly on platform service fees before disbursement.
   - Deduct GCT on the gross booking amount.
   *What is the exact tax calculation model approved for Jamaican host payouts?*
""",
        "auto-triage-incident:Telemetry to Intent Artifact Test": """
# Incident Triage: DB Deadlock on Availability Locks

**Severity**: P0 - Critical  
**Target Artifact**: docs/intent/intent-20260908-01-booking-deadlock.md

### Problem Evidence
```
ERROR booking_api: deadlock detected on property_availability row locks during concurrent hold initiation for villa_id=987
```

### Root Cause Hypothesis
Concurrent requests for overlapping date windows on `villa_id=987` acquire row-level locks out of chronological order, triggering cyclic wait-for deadlocks between Postgres transactions.

### Guardrails Check
- Invariant: Zero double bookings via strict PostgreSQL row-level locks.
"""
    }

def main():
    benchmarks_data = load_benchmarks()
    benchmarks = benchmarks_data.get("benchmarks", [])
    
    if len(sys.argv) > 1 and sys.argv[1] == "--list":
        print(f"=== Registered Skill Benchmarks ({len(benchmarks)}) ===")
        for idx, b in enumerate(benchmarks, 1):
            print(f"{idx}. [{b['skill_id']}] {b['name']}")
            print(f"   Prompt: {b['prompt']}")
            print(f"   Assertions: {len(b.get('assertions', []))}")
        return

    if len(sys.argv) > 1 and sys.argv[1] == "--verify":
        print(f"=== Executing Benchmark Assertions ({len(benchmarks)} suites) ===\n")
        exemplars = get_exemplars()
        total_assertions = 0
        total_passed = 0
        
        for idx, b in enumerate(benchmarks, 1):
            key = f"{b['skill_id']}:{b['name']}"
            sample = exemplars.get(key, "")
            results = run_assertions(b, sample)
            
            passed_count = sum(1 for r in results if r["passed"])
            all_passed = passed_count == len(results)
            total_assertions += len(results)
            total_passed += passed_count
            
            status = "PASS" if all_passed else "FAIL"
            print(f"[{status}] {idx}. [{b['skill_id']}] {b['name']} ({passed_count}/{len(results)} assertions passed)")
            for r in results:
                icon = "  ✓" if r["passed"] else "  ✗"
                print(f"    {icon} {r['reason']}")
            print()
            
        pass_rate = (total_passed / total_assertions * 100) if total_assertions else 0
        print(f"=== Summary: {total_passed}/{total_assertions} assertions passed ({pass_rate:.1f}%) ===")
        if total_passed < total_assertions:
            sys.exit(1)
        return

    print(f"Loaded {len(benchmarks)} skill benchmarks from {BENCHMARK_FILE.name}")
    print("Options: --list to view benchmarks, --verify to execute assertion test matrix.")

if __name__ == "__main__":
    main()
