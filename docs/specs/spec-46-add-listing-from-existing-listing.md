# Spec 46: Add Listing from Existing Listing

## Overview
This feature adds the ability for administrators to quickly create a new villa listing by prepopulating the "Add New Listing" form with details from an existing listing found in search results.

## Requirements

### Flow
1. **Sign In**: User must be signed in with administrator privileges to access `web_app_admin`.
2. **Search**: On the Listings page, administrators can search for existing villas using the search filters.
3. **Populate**: Each search result will display a button (e.g., "Use as Template" or "Copy Details").
4. **Pre-fill**: Clicking this button will populate all fields in the "Add New Listing" form with the values from the selected listing, **except for the Listing Name**.

### Exclusions
- **Listing Name**: This field must NOT be populated. It should remain empty (or keep its current value) to ensure the administrator provides a unique name for the new entry.

### Included Fields
The following fields should be populated from the source listing:
- Owner Email (and trigger validation/retrieval of Owner ID)
- Description
- Structure Type (Apartment, House, Villa, etc.)
- Country
- Base Currency
- Price Per Night
- Weekly/Monthly Discounts
- Latitude/Longitude
- Capacity (Max Guests, Bedrooms, Beds)
- Bathrooms (Full/Half)
- Square Meters
- Minimum Stay & Days Between Bookings
- Listing Details (Key-Value pairs like WiFi, Pool, etc.)

## Technical Implementation

### Module: `web_app_admin`
- **File**: `web_app_admin/src/components/listings.rs`
- **Component**: `ListingsPage`

### Implementation Details
1. **State Management**:
   - Create a mechanism to update the signals/inputs used by the `ActionForm` for "Add New Listing".
   - Since the "Add New Listing" form uses an `ActionForm` which fetches data from the DOM on submit, we need to ensure the `prop:value` or `value` of the inputs is updated when the "Populate" button is clicked.

2. **UI Changes**:
   - Update the search result card in the `For` loop (lines 496-517 in `listings.rs`) to include a new button next to the "View" button.
   - Add a click handler to this button that takes the `listing` (of type `ListingResponse`) and updates the form state.

3. **Owner Validation**:
   - When populating the Owner Email, trigger the `on_email_input` logic or manually call `get_user_server` to ensure the `owner_id_validated` signal is set, allowing the form to be submitted.

4. **Listing Details**:
   - Correctily map the `listing_details` JSON from the existing listing back into the `Vec<(usize, String, String)>` state used by the dynamic detail inputs.

## Acceptance Criteria
- [ ] Sign in to Admin App works.
- [ ] Searching for villas returns results with a "Populate" button.
- [ ] Clicking "Populate" fills common fields (Country, Price, etc.).
- [ ] Clicking "Populate" does NOT fill the "Listing Name" field.
- [ ] Clicking "Populate" fills the "Owner Email" and validates it.
- [ ] Clicking "Populate" fills the "Listing Details" (WiFi, etc.).
- [ ] The user can then modify the fields and successfully create a new listing.
