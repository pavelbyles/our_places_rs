# Spec 47: Update Check-in Times

## 1. Overview
Update the "Estimated Arrival Time" (check-in) options in the `web_app` checkout process to provide revised time windows for guests. The goal is to align the selection options with the new operational requirements.

## 2. Requirements
The "Estimated Arrival Time" dropdown in the checkout page must be updated to include the following time slots:

| Display Label | Value (24h) |
| :--- | :--- |
| 9:00 AM – 11:00 AM | `09:00` |
| 11:00 AM – 1:00 PM | `11:00` |
| 1:00 PM – 3:00 PM | `13:00` |
| 3:00 PM – 5:00 PM | `15:00` |
| 5:00 PM – 7:00 PM | `17:00` |
| 7:00 PM – 12:00 AM | `19:00` |

## 3. Affected Components

### Frontend (web_app)
- **File**: `web_app/src/components/checkout.rs`
- **Component**: `CheckoutPage`
- **Logic**: Update the `<select>` element that binds to the `arrival_time` signal. Replace the existing hardcoded `<option>` elements with the new requirements.

## 4. Implementation Details

The current implementation in `web_app/src/components/checkout.rs` uses 1-hour slots:
```rust
<select class="select select-bordered" ...>
    <option value="">"Select a time"</option>
    <option value="09:00">"09:00 AM – 10:00 AM"</option>
    <option value="12:00">"12:00 PM – 01:00 PM"</option>
    ...
</select>
```

The updated version should look like this:
```rust
<select class="select select-bordered" ...>
    <option value="">"Select a time"</option>
    <option value="09:00">"09:00 AM – 11:00 AM"</option>
    <option value="11:00">"11:00 AM – 01:00 PM"</option>
    <option value="13:00">"01:00 PM – 03:00 PM"</option>
    <option value="15:00">"03:00 PM – 05:00 PM"</option>
    <option value="17:00">"05:00 PM – 07:00 PM"</option>
    <option value="19:00">"07:00 PM – 12:00 AM"</option>
</select>
```

## 5. Verification Plan
1. **Visual Check**: Open the checkout page for any listing and ensure the dropdown shows the correct 2-hour windows (and the final 5-hour window).
2. **Functional Check**: Select the "7:00 PM – 12:00 AM" option and complete a test booking. Verify the value `19:00` is correctly passed to the backend and stored in the booking metadata.
3. **Regression**: Ensure that previously selected arrival times in existing bookings (if any) are still displayed correctly or handled gracefully.
