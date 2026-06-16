# Spec 39: Add Currency to Listing and User

## Overview
This feature introduces currency support across the platform. It allows a listing to have a default currency, which is displayed to non-logged-in guests. Additionally, users (travellers/guests) can set their preferred default currency on their profile. A currency mapping mechanism is introduced to convert listing prices to the user's preferred currency when viewing listings or making bookings.

## Requirements

### Listings
- **Default Currency**: Each listing must have a default currency.
- **Creation**: The listing currency is initially set when the listing is created.
- **Modification**: The currency can be modified by the host or an administrator when updating the listing.
- **Guest View**: When non-logged-in guests view a listing, prices should be displayed in the listing's default currency.

### Users
- **Preferred Currency**: Users (including travellers and hosts) can choose a default currency from their profile settings.
- **Logged-in View**: When a user is logged in, prices across the platform (listings, checkout, bookings) should be displayed in their chosen default currency.

### Currency Conversion
- **Mapping Table**: Add a currency mapping table to store exchange rates and perform conversions between listing currencies and user default currencies.
- **Booking Flow**: When a booking is being made, the price must be converted from the listing's base currency to the user's preferred currency to display the correct amount.

## Technical Implementation

### Database Changes
1. **`listings` Table**: Add a `currency` column (e.g., VARCHAR or ENUM) to store the listing's base currency.
2. **`users` Table**: Add a `default_currency` column to store the user's preferred currency.
3. **`currency_rates` Table**: Create a new table to store currency exchange rates (e.g., `id`, `base_currency`, `target_currency`, `exchange_rate`, `last_updated`).

### Backend (APIs & Core)
- **Migrations**: Create SQL migrations for the new columns and the currency mapping table.
- **Listing API**: Update listing creation and update endpoints to process the `currency` field.
- **User API**: Update the profile update endpoint to accept and save the `default_currency` field.
- **Conversion Logic**: Implement a service that retrieves the current exchange rate from the `currency_rates` table and computes the converted price when fetching listings or calculating booking totals for a logged-in user.

### Frontend
- **Add/Edit Listing**: Add a currency selector dropdown to the listing creation and edit forms.
- **User Profile**: Add a currency preference selector to the user's profile settings.
- **Price Display**: Ensure that listing prices and booking totals dynamically format and display the correct currency symbol and converted amount based on the logged-in status and user preferences.

## Acceptance Criteria
- [ ] Database migrations for listings, users, and the currency mapping table are created and applied.
- [ ] A listing's base currency can be set during creation and modified during updates.
- [ ] Users can view and update their preferred default currency in their profile.
- [ ] Non-logged-in users view listing prices in the listing's base currency.
- [ ] Logged-in users view listing prices converted to their preferred currency on the listing page.
- [ ] During the booking process, prices are correctly converted and displayed in the user's preferred currency.
- [ ] The currency mapping table is used to accurately perform the conversions.
