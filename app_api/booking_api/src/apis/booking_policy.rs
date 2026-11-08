//! Which booking party may change which part of a booking.
//!
//! Whether a status move is allowed at all (e.g. a cancelled booking can never
//! be confirmed again) is enforced in `db_core` under the row lock; this module
//! only decides *who* may ask for it.

use super::booking_updates::UpdatedBookingRequest;
use db_core::models::BookingStatus;

/// The caller's relationship to one booking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BookingRole {
    Guest,
    Host,
    Admin,
}

/// Checks `req` against the caller's role. `Err` is the reason for a 403.
///
/// - Anyone on the booking may cancel it.
/// - Confirming: the listing's host or an admin. It stands in for payment
///   confirmation until a payment webhook exists; `db_core` only allows it
///   from `pending`. Completing is done by the stay-completion cron (or an
///   admin).
/// - Door codes and metadata (guest counts, notes, business tax details):
///   host or admin. Direct date/guest-count changes, which reprice the stay:
///   admin only; guests and hosts file a reservation change request instead.
pub(super) fn authorize_update(
    role: BookingRole,
    req: &UpdatedBookingRequest,
) -> Result<(), &'static str> {
    if role == BookingRole::Admin {
        return Ok(());
    }
    match req.status {
        Some(BookingStatus::Confirmed) if role != BookingRole::Host => {
            return Err("Only the host or an administrator can confirm bookings");
        }
        Some(BookingStatus::Completed) => {
            return Err("Only administrators can mark bookings completed");
        }
        Some(BookingStatus::Pending) => {
            return Err("Only administrators can set a booking back to pending");
        }
        Some(BookingStatus::Confirmed | BookingStatus::Cancelled) | None => {}
    }
    if req.door_access_code.is_some() && role != BookingRole::Host {
        return Err("Only hosts and administrators can configure property door access codes");
    }
    if req.metadata.is_some() && role != BookingRole::Host {
        return Err("Only hosts and administrators can change booking details");
    }
    if req.date_from.is_some() || req.date_to.is_some() || req.number_of_persons.is_some() {
        return Err(
            "Direct reservation date or guest count changes require administrator privileges; submit a reservation change request instead",
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use BookingRole::*;

    fn status(s: BookingStatus) -> UpdatedBookingRequest {
        UpdatedBookingRequest {
            status: Some(s),
            ..UpdatedBookingRequest::default()
        }
    }

    #[test]
    fn every_party_may_cancel() {
        for role in [Guest, Host, Admin] {
            assert!(authorize_update(role, &status(BookingStatus::Cancelled)).is_ok());
        }
    }

    #[test]
    fn hosts_and_admins_confirm_guests_do_not() {
        let req = status(BookingStatus::Confirmed);
        assert!(authorize_update(Guest, &req).is_err());
        assert!(authorize_update(Host, &req).is_ok());
        assert!(authorize_update(Admin, &req).is_ok());
    }

    #[test]
    fn only_admins_complete_or_reopen() {
        for s in [BookingStatus::Completed, BookingStatus::Pending] {
            assert!(
                authorize_update(Guest, &status(s)).is_err(),
                "guest -> {s:?}"
            );
            assert!(authorize_update(Host, &status(s)).is_err(), "host -> {s:?}");
            assert!(
                authorize_update(Admin, &status(s)).is_ok(),
                "admin -> {s:?}"
            );
        }
    }

    #[test]
    fn door_codes_are_for_hosts_and_admins() {
        let req = UpdatedBookingRequest {
            door_access_code: Some("1234".to_string()),
            ..UpdatedBookingRequest::default()
        };
        assert!(authorize_update(Guest, &req).is_err());
        assert!(authorize_update(Host, &req).is_ok());
        assert!(authorize_update(Admin, &req).is_ok());
    }

    #[test]
    fn metadata_is_for_hosts_and_admins() {
        let req = UpdatedBookingRequest {
            metadata: Some(db_core::models::BookingMetadata {
                num_adults: 2,
                num_children: 0,
                num_infants: 0,
                num_pets: 0,
                message_to_host: None,
                estimated_arrival_time: None,
                is_business_trip: true,
                business_company_name: Some("Acme".to_string()),
                business_tax_id: Some("123-456-789".to_string()),
            }),
            ..UpdatedBookingRequest::default()
        };
        assert!(authorize_update(Guest, &req).is_err());
        assert!(authorize_update(Host, &req).is_ok());
        assert!(authorize_update(Admin, &req).is_ok());
    }

    #[test]
    fn terms_are_admin_only() {
        let dates = UpdatedBookingRequest {
            date_from: chrono::NaiveDate::from_ymd_opt(2030, 1, 1),
            ..UpdatedBookingRequest::default()
        };
        let guests = UpdatedBookingRequest {
            number_of_persons: Some(3),
            ..UpdatedBookingRequest::default()
        };
        for req in [dates, guests] {
            assert!(authorize_update(Guest, &req).is_err());
            assert!(authorize_update(Host, &req).is_err());
            assert!(authorize_update(Admin, &req).is_ok());
        }
    }
}
