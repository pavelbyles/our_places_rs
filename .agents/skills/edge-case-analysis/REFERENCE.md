# Edge Case Analysis Reference

## 1. Domain Edge Case Categories

### 1. Financial & Currency Mathematics
- [ ] Rounding precision drift in multi-currency conversion (Tri-currency flow: Base $\rightarrow$ Checkout $\rightarrow$ Tax).
- [ ] Sub-cent fractional rounding (`rust_decimal` Decimal precision).
- [ ] Extreme exchange rates (e.g. 0.00001 or 100,000.0).
- [ ] Zero-dollar coupons / 100% discount promo codes.

### 2. Dates, Times & Booking Windows
- [ ] Check-in and check-out on the same day (invalid duration).
- [ ] Check-in across midnight in different timezones (`chrono::DateTime<Utc>`).
- [ ] Leap year (Feb 29) booking ranges and daylight savings transitions.
- [ ] Hold expiration occurring precisely at the millisecond of payment submission.

### 3. Strings & Payload Extremes
- [ ] Special character injection in villa search / titles (`'`, `"`, `<script>`, SQL meta-characters).
- [ ] Zero-length strings vs whitespace-only strings.
- [ ] Multi-byte UTF-8 emoji and RTL language rendering.
- [ ] Payloads at maximum byte limits (e.g. 10MB image uploads).
