# Spec 50: Resend Login Verification Code

## Overview
This feature allows a user who didn't receive their initial verification code (or if the code expired) to request a new verification code. Generating a new verification code will invalidate any previous valid verification code.

## Requirements

### Flow
1. **Request Resend**: A user submits a request with their email address to resend the verification code.
2. **User Validation**: The system checks if a user exists with that email. If the user is already verified, the system should return an appropriate error message.
3. **Invalidation**: Any existing verification code for the user is replaced/invalidated.
4. **Generation**: A new 6-character alphanumeric OTP is generated.
5. **Expiration**: The new code is given a 30-minute expiration timestamp.
6. **Delivery**: The verification code is printed to the server logs (and would be sent via email in a complete implementation).

## Edge Cases
- **Non-existent Email**: If an email is requested that does not exist in the database, the API should return a standard error without exposing whether the email is registered (to prevent enumeration attacks) or return a safe generic success message.
- **Already Verified User**: Requesting a resend for a user who is already verified should return a `400 Bad Request` or similar error.
- **Rate Limiting**: To prevent abuse, requests to resend codes should be rate-limited per email or IP address.
- **Concurrent Requests**: If a user submits the request twice rapidly, the database update must be atomic so only one valid code results and no race condition occurs.

## Technical Implementation

### Module: `app_api/user_api`
- **File**: `app_api/user_api/src/apis.rs`
- **Component**: New `resend_verification` endpoint
- **Structs**: New `ResendVerificationRequest` containing `pub email: String`.

### Module: `db_core`
- **File**: `db_core/src/user.rs`
- **Component**: Existing user update methods

### Implementation Details
1. **API Endpoint**:
   - Add a new endpoint `POST /api/v1/users/resend-verification`.
   - The endpoint should accept a `ResendVerificationRequest`.
   - Validate the request using `validator::Validate`.

2. **Database Logic**:
   - Fetch the user by email using `db_core::user::get_user_by_email`.
   - Check if `!user.is_verified`. If already verified, return an error (e.g. `400 Bad Request` - "User is already verified").
   - Generate a new `otp` string using `Alphanumeric.sample_string(&mut rand::rng(), 6).to_uppercase()`.
   - Trace log the new verification code using `tracing::info!` for local development/testing.
   - Update the user with the new `verification_code` and `verification_code_expires_at` (set to `Utc::now() + chrono::Duration::minutes(30)`). Use `db_core::user::update_user` with an `UpdatedUser` struct, or add a specific database function if needed.

3. **Utoipa OpenAPI Definitions**:
   - Add the `ResendVerificationRequest` to the schema definitions in `configure_routes`.
   - Add the `resend_verification` path to the `#[openapi(paths(...))]` macro.
   - Wire the new route in the `web::scope("/api/v1/users")` configuration.

### Module: `web_app`
- **File**: `web_app/src/auth.rs`
- **Component**: New `resend_verification_code` ServerFn
- **File**: `web_app/src/components/verify.rs`
- **Component**: `VerifyPage` UI Updates

### Frontend Implementation Details
1. **Server Function (`auth.rs`)**:
   - Create a new `#[server]` function `resend_verification_code(email: String) -> Result<(), ServerFnError>`.
   - Inside the `#[cfg(feature = "ssr")]` block, build the payload and use `get_client().post(...)` to call the new `/api/v1/users/resend-verification` endpoint.
   - Handle the API response and map HTTP errors into `ServerFnError`.

2. **UI Updates (`verify.rs`)**:
   - Initialize a `ServerAction::<ResendVerificationCode>::new()` to manage the action state.
   - Replace the `(Not implemented)` placeholder button with a functional button that triggers this action (e.g. by wrapping it in an `ActionForm`).
   - Provide visual feedback: display a success message (e.g., "Verification code resent!") or an error alert based on the action result.
   - Disable the button and change its text to "Sending..." while the action is pending.

## Unit Test Cases
1. `test_resend_verification_success`: Verify that a valid unverified user gets a new code, the old code is invalidated, and the new code works.
2. `test_resend_verification_already_verified`: Verify that requesting a code for an already verified user returns an error.
3. `test_resend_verification_nonexistent_user`: Verify that requesting a code for a non-existent email returns the expected safe response.
4. `test_resend_verification_invalidates_old_code`: Explicitly verify that an old code is rejected after a resend is requested.

## Acceptance Criteria
- [ ] New `POST /api/v1/users/resend-verification` endpoint is exposed and documented in Swagger.
- [ ] The endpoint accepts a JSON body with an `email` field.
- [ ] If the user does not exist or is already verified, an appropriate error is returned.
- [ ] If successful, a new OTP is generated, logged, and updated in the database with a 30-minute expiration.
- [ ] The previous verification code (if any) is overwritten and effectively invalidated.
- [ ] A user can successfully verify their account using the *new* verification code.
- [ ] Using the *old* verification code fails.
- [ ] The "Resend Code" button on the frontend `VerifyPage` successfully triggers the backend API.
- [ ] The `VerifyPage` provides appropriate visual feedback (loading state, success message, error message) when resending the code.
