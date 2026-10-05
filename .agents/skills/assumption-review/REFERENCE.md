# Assumption Review Reference

## 1. Hidden Assumption Taxonomy

| Domain | Commonly Held (Dangerous) Assumption | Reality / Failure Trigger |
| :--- | :--- | :--- |
| **Network & I/O** | External exchange rate API is always fast (<100ms) and up. | API throttles, times out (10s), or returns unexpected schema. |
| **Concurrency** | Users will not click "Book Now" at the same sub-second. | Double-booking occurs without atomic row locks (`FOR UPDATE`). |
| **State** | Guest users always finish checkout within 5 minutes. | Users abandon carts, requiring automated hold release after 15m. |
| **Storage** | Image worker always finishes resize before frontend requests thumbnail. | Frontend requests image while still processing; needs fallback URL. |
| **Scale** | Cloud Run instances are always warm. | Cold starts (up to 1s) cause latency spikes if init is synchronous. |

---

## 2. Socratic Assumption Challenge Prompts

1. *"What is the single biggest unverified assumption this design relies on?"*
2. *"If that component or third-party service is completely down for 2 hours, what does the user experience?"*
3. *"Is there a statutory or contractual requirement behind this constraint, or is it an arbitrary default?"*
4. *"Can we prove this assumption with an integration test or benchmark before writing production code?"*
