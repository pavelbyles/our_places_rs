# Spec 54: Add user profile features

## Overview
This feature adds user profile management capabilities, allowing users to update their password, change their email address, and deactivate their account. Additionally, it implements a default system admin account with default credentials that must be initialized and can only be changed by the admin themselves. 

## Requirements

### Flow

1. **System Admin Initialization:**
   - On application startup, a Rust function called in `user_api/src/main.rs` after `run_migrations` safely hashes the password using **bcrypt** (consistent with the rest of the codebase) and ensures the default system admin user exists.
   - The default credentials (e.g., `pavelbyles@ourplaces.io`) are established.
   - The password can only be updated by the admin themselves.
   - The system admin user must **not** appear in the admin panel's user list. The `get_all_users` query and admin panel should exclude users with `UserRole::Admin`.

2. **Update Password:**
   - User requests a password change by providing their **current password** and email.
   - The system verifies the current password is correct.
   - The system generates a verification code and sends a **password change 2FA email** (a distinct email template from the existing account verification email) indicating the password is being changed.
   - The user must provide the verification code along with their new password to finalize the change.

3. **Change Email Address:**
   - User requests an email change.
   - The system validates the user's current password.
   - If the password is correct, the email is updated. 
   - If the new email is already taken by another user, the system returns a user-friendly `400 Bad Request` error (e.g., "Email is already in use").
   - The user's `is_verified` flag is set to `false` and a new verification code is generated, requiring them to verify the new email.
   - The user's existing session is **purged** and they are redirected to the verification page for the new email.

4. **Deactivate Account:**
   - User requests account deactivation.
   - The system sets the user's `is_active` flag to `false`.
   - Any listings belonging to this user are filtered out of result sets (search, explore, etc.) preventing new bookings.
   - Existing active or upcoming bookings (either as guest or host) are kept active.
   - The user's existing session is **purged** and they are redirected to the homepage.

## Edge Cases
- **Concurrent Requests:** Rapid duplicate requests for email changes or password resets must be handled gracefully to avoid race conditions.
- **Incorrect Password:** If the current password validation fails during an email change or password change request, the system must return a `400 Bad Request` or `401 Unauthorized`.
- **Invalid/Expired 2FA Code:** If the verification code provided during a password update is incorrect or expired, the update must fail with an appropriate error.
- **Admin Credential Protection:** The system admin (`pavelbyles@ourplaces.io`) must be immutable through the API. The `update_user` endpoint must be guarded by middleware that rejects **any** modification to a user with `UserRole::Admin`, returning `403 Forbidden`. The system admin's credentials are managed manually (e.g., direct database access), never through the web application or admin panel.
- **Deactivated User Login:** A deactivated user should not be able to log in. The login flow must check the `is_active` flag and reject authentication if deactivated.
- **Duplicate Email on Change:** If the user attempts to change their email to one already registered, the system must return a clear, user-friendly error without exposing internal details.

## Technical Implementation

### Module: `db_core`
- **File**: `db_core/src/user.rs`
  - **Component**: Existing user methods.
  - Implement a mechanism (e.g. `initialize_system_admin` function) that creates a default system admin with `UserRole::Admin` using **bcrypt** password hashing (matching the existing codebase pattern).
  - Ensure update functions can handle password and email updates safely, including resetting `is_verified` to `false` on email change.
- **File**: `db_core/src/listing.rs`
  - **Component**: Listing query functions.
  - Add `AND "user".is_active = TRUE` filter to **all** listing queries that surface results to users:
    - `get_listings` — already joins `user`, add the filter to the WHERE clause.
    - `get_listing_by_id` — add a JOIN to `user` and filter by `is_active = TRUE`.
    - `get_listing_by_id_or_slug` — add a JOIN to `user` and filter by `is_active = TRUE`.
    - `get_listings_by_user_id` — consider whether the owner should see their own listings even if deactivated (for re-activation scenarios); if not, add the filter.

### Module: `app_api/user_api`
- **File**: `app_api/user_api/src/apis.rs`
  - **Component**: New endpoints for profile management.
  - `POST /api/v1/users/profile/password/request` -> Accepts `email` and `current_password`, verifies the current password, generates a verification code, saves it to the user record, and logs/sends a **password change 2FA email**.
  - `POST /api/v1/users/profile/password/confirm` -> Validates 2FA code and updates password hash (using bcrypt).
  - `POST /api/v1/users/profile/email` -> Accepts `current_password` and `new_email`, validates password, checks new email uniqueness, updates email, resets `is_verified` to `false`, generates a new verification code, and sends a verification email to the new address.
  - `POST /api/v1/users/profile/deactivate` -> Sets `is_active` to false for the requesting user.
  - **Modify the `login` endpoint** to check `is_active` and reject authentication with an appropriate error (e.g., "Account has been deactivated") if `is_active = false`.
  - **Modify the `get_all_users` endpoint** to exclude users with `UserRole::Admin` from the results so the system admin does not appear in the admin panel.
  - **Add admin guard middleware** to the `update_user` (`PATCH /api/v1/users/user/{id}`) endpoint. The middleware must:
    1. Fetch the target user by ID.
    2. Check if the target user has `UserRole::Admin` in their roles.
    3. If yes, **unconditionally reject** the request with `403 Forbidden`. No caller — including the admin themselves — can modify the system admin via the API.
    The system admin (`pavelbyles@ourplaces.io`) is managed manually via direct database access.

### Module: `web_app_common`
- **File**: `web_app_common/src/email.rs`
  - **Component**: New email function.
  - Add a new `send_password_change_email` function (and corresponding HTML template in `web_app_common/templates/`) distinct from `send_verification_email`. This email should clearly communicate that a password change was requested and include the 2FA code.

### Module: `web_app`
- **File**: `web_app/src/auth.rs`
  - **Component**: New ServerFns: `request_password_change`, `confirm_password_change`, `change_email`, `deactivate_account`.
  - `change_email` ServerFn must **purge the session** after a successful email change and redirect to the verification page for the new email.
  - `deactivate_account` ServerFn must **purge the session** and redirect to the homepage.
- **File**: `web_app/src/components/profile.rs` (or equivalent profile management component)
  - **Component**: UI updates to support password, email changes, and account deactivation. Includes necessary forms, client-side validation, and visual feedback (success/error alerts).

### Module: `web_app_admin`
- **File**: `web_app_admin/src/components/user.rs`
  - **Component**: User list filtering.
  - The system admin user must not appear in the admin panel's user list. This is enforced by the API-level exclusion of `UserRole::Admin` users from `get_all_users`.

## Unit Test Cases
1. `test_system_admin_initialized`: Verify that the default system admin is created if not present on startup.
2. `test_admin_idempotent_init`: Verify that running `initialize_system_admin` multiple times does not create duplicates or error.
3. `test_update_password_flow`: Verify that requesting a password change with a valid current password generates a code, and submitting the code with a new password updates it successfully.
4. `test_password_change_invalid_current_password`: Verify that requesting a password change with an incorrect current password is rejected.
5. `test_password_change_invalid_code`: Verify that submitting an incorrect or expired 2FA code during password confirmation fails with an appropriate error.
6. `test_change_email_success`: Verify that providing the correct current password updates the user's email.
7. `test_change_email_resets_verification`: Verify that after a successful email change, `is_verified` is set to `false` and a new verification code is generated.
8. `test_change_email_invalid_password`: Verify that an incorrect password rejects the email change.
9. `test_change_email_duplicate`: Verify that changing to an email already taken by another user returns a user-friendly error.
10. `test_deactivate_account_hides_listings`: Verify that after deactivating an account, their listings do not appear in `get_listings` results.
11. `test_deactivate_hides_listing_by_id`: Verify that `get_listing_by_id` and `get_listing_by_id_or_slug` return errors for listings owned by deactivated users.
12. `test_deactivated_user_cannot_login`: Verify that a deactivated user receives an error when attempting to log in.
13. `test_admin_not_in_user_list`: Verify that the system admin user does not appear in `get_all_users` results.
14. `test_admin_cannot_be_modified`: Verify that the `update_user` endpoint returns `403 Forbidden` when **any** caller attempts to modify the system admin user (`pavelbyles@ourplaces.io`).

## Acceptance Criteria
- [ ] System admin is automatically initialized with default credentials if none exist.
- [ ] System admin does not appear in the admin panel user list.
- [ ] New `POST` endpoints for password request, password confirm, email change, and account deactivation are exposed and documented in Swagger.
- [ ] Password change request requires the current password before generating a 2FA code.
- [ ] A distinct password change email template is used for the 2FA code (not the account verification template).
- [ ] Password updates successfully require a 2FA verification code.
- [ ] Email changes successfully require validating the current password.
- [ ] Attempting to change email to one already in use returns a user-friendly error.
- [ ] Email change resets `is_verified` to false and requires re-verification.
- [ ] Session is purged after email change and account deactivation.
- [ ] Account deactivation sets `is_active` to false.
- [ ] Deactivated users cannot log in.
- [ ] Listings owned by deactivated users do not show up in any search, detail, or listing result sets (`get_listings`, `get_listing_by_id`, `get_listing_by_id_or_slug`).
- [ ] API middleware on `update_user` unconditionally rejects all modifications to the system admin (`pavelbyles@ourplaces.io`) with `403 Forbidden`.
- [ ] Frontend UI supports all profile actions with appropriate loading states and error/success feedback.
