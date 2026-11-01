//! Route table and OpenAPI document for booking_api.

use super::availability::{AvailabilityResponse, check_availability, get_unavailable_ranges};
use super::booking_updates::{
    UpdatedBookingRequest, delete_booking, transfer_booking, update_booking,
};
use super::bookings::{
    create_booking, get_booking_by_id, get_bookings, get_listing_bookings, get_user_bookings,
};
use super::cron::{
    CompleteStaysResponse, CronSweepResponse, CronSweepsResponse, complete_stays_cron,
    process_scheduled_notifications, run_sweeps,
};
use super::documents::{
    get_booking_document_by_number, get_booking_documents, get_cancellation_quote,
};
use super::ledger::{
    export_host_ledger_csv, get_admin_exchange_rates, get_host_ledger_entries,
    get_host_ledger_summary, update_admin_payout_status,
};
use super::messages::{get_booking_messages, mark_booking_messages_read, send_booking_message};
use super::reservation_changes::{
    approve_reservation_change_request, cancel_reservation_change_request,
    create_reservation_change_request, decline_reservation_change_request,
    list_reservation_change_requests, quote_reservation_change,
};
use super::{
    availability, booking_updates, bookings, cron, documents, ledger, messages, reservation_changes,
};
use actix_web::middleware::from_fn;
use actix_web::web;
use api_core::api_common::content_negotiation_middleware;
use api_core::auth::require_user;
use api_core::{models::BookingResponse, pagination};
use common::models::NewBookingRequest;
use db_core::models::{BookingStatus, CancellationPolicy, FeeItem};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

/// Configures Actix-web HTTP routes and OpenAPI specifications for the Booking service.
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    #[derive(OpenApi)]
    #[openapi(
        paths(
            availability::check_availability,
            bookings::create_booking,
            bookings::get_bookings,
            bookings::get_booking_by_id,
            bookings::get_user_bookings,
            bookings::get_listing_bookings,
            availability::get_unavailable_ranges,
            ledger::get_admin_exchange_rates,
            booking_updates::update_booking,
            booking_updates::delete_booking,
            booking_updates::transfer_booking,
            messages::get_booking_messages,
            messages::send_booking_message,
            messages::mark_booking_messages_read,
            cron::process_scheduled_notifications,
            documents::get_cancellation_quote,
            reservation_changes::quote_reservation_change,
            reservation_changes::create_reservation_change_request,
            reservation_changes::list_reservation_change_requests,
            reservation_changes::approve_reservation_change_request,
            reservation_changes::decline_reservation_change_request,
            reservation_changes::cancel_reservation_change_request,
            documents::get_booking_documents,
            documents::get_booking_document_by_number,
            cron::complete_stays_cron,
            cron::run_sweeps,
        ),
        components(
            schemas(
                NewBookingRequest,
                UpdatedBookingRequest,
                common::models::TransferBookingRequest,
                AvailabilityResponse,
                common::availability::UnavailableRange,
                common::availability::UnavailableRangesResponse,
                common::exchange_rate::ExchangeRate,
                common::exchange_rate::ExchangeRatesResponse,
                BookingResponse,
                pagination::Pagination,
                FeeItem,
                BookingStatus,
                CancellationPolicy,
                common::models::BookingMessageResponse,
                common::models::BookingMessagesWrapper,
                common::models::CreateBookingMessageRequest,
                common::models::MarkMessagesReadResponse,
                CronSweepResponse,
                CompleteStaysResponse,
                CronSweepsResponse,
                common::cancellation::RefundQuote,
                common::models::ReservationChangeStatus,
                common::models::ChangeInitiatorRole,
                common::models::ChangeSettlementStatus,
                common::models::ReservationChangePricingSnapshot,
                common::models::ReservationChangeQuoteRequest,
                common::models::ReservationChangeQuoteResponse,
                common::models::CreateReservationChangeRequest,
                common::models::RespondReservationChangeRequest,
                common::models::ReservationChangeRequestResponse,
                common::tax_document::TaxDocumentResponse,
                common::tax_document::TaxDocumentType,
                common::tax_document::TaxDocumentStatus,
                common::tax_document::InvoiceLineItem,
                common::tax_document::InvoiceTaxCategory,
            )
        ),
        tags(
            (name = "bookings", description = "Booking management endpoints")
        ),
    )]
    struct ApiDoc;

    // Register Swagger UI services at the ROOT scope so paths match
    cfg.service(
        SwaggerUi::new("/api/docs/swagger-ui/{_:.*}")
            .url("/api-docs/openapi.json", ApiDoc::openapi()),
    );

    // Public: anyone browsing a villa may check dates. These are registered
    // before the protected scope below, which matches the same prefix.
    cfg.service(
        web::resource("/api/v1/bookings/availability").route(
            web::get()
                .to(check_availability)
                .wrap(from_fn(content_negotiation_middleware)),
        ),
    );
    cfg.service(
        web::resource("/api/v1/bookings/listing/{id}/unavailable-ranges")
            .route(web::get().to(get_unavailable_ranges)),
    );

    // Everything else needs a signed-in user; handlers then check that the
    // user may act on the specific booking.
    cfg.service(
        web::scope("/api/v1/bookings")
            .wrap(from_fn(require_user))
            .route(
                "",
                web::get()
                    .to(get_bookings)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "",
                web::post()
                    .to(create_booking)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/user/{id}",
                web::get()
                    .to(get_user_bookings)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/listing/{id}",
                web::get()
                    .to(get_listing_bookings)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}",
                web::get()
                    .to(get_booking_by_id)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}",
                web::patch()
                    .to(update_booking)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}",
                web::delete()
                    .to(delete_booking)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}/transfer",
                web::post()
                    .to(transfer_booking)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}/cancellation-quote",
                web::get()
                    .to(get_cancellation_quote)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}/change-quote",
                web::post()
                    .to(quote_reservation_change)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}/change-requests",
                web::post()
                    .to(create_reservation_change_request)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}/change-requests",
                web::get()
                    .to(list_reservation_change_requests)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}/change-requests/{request_id}/approve",
                web::post()
                    .to(approve_reservation_change_request)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}/change-requests/{request_id}/decline",
                web::post()
                    .to(decline_reservation_change_request)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}/change-requests/{request_id}/cancel",
                web::post()
                    .to(cancel_reservation_change_request)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}/documents",
                web::get()
                    .to(get_booking_documents)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}/documents/{number}",
                web::get()
                    .to(get_booking_document_by_number)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}/messages",
                web::get()
                    .to(get_booking_messages)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}/messages",
                web::post()
                    .to(send_booking_message)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/{id}/messages/read",
                web::patch()
                    .to(mark_booking_messages_read)
                    .wrap(from_fn(content_negotiation_middleware)),
            ),
    );

    cfg.service(
        web::scope("/api/v1/hosts/ledger")
            .wrap(from_fn(require_user))
            .route(
                "",
                web::get()
                    .to(get_host_ledger_entries)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/summary",
                web::get()
                    .to(get_host_ledger_summary)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route("/export", web::get().to(export_host_ledger_csv)),
    );

    cfg.service(
        web::scope("/api/v1/admin/exchange-rates")
            .wrap(from_fn(require_user))
            .route("", web::get().to(get_admin_exchange_rates)),
    );

    cfg.service(
        web::scope("/api/v1/admin/ledger")
            .wrap(from_fn(require_user))
            .route(
                "/{id}/status",
                web::patch()
                    .to(update_admin_payout_status)
                    .wrap(from_fn(content_negotiation_middleware)),
            ),
    );

    cfg.service(
        web::scope("/api/v1/internal/cron")
            .route(
                "/process-scheduled-notifications",
                web::post().to(process_scheduled_notifications),
            )
            .route("/complete-stays", web::patch().to(complete_stays_cron))
            .route("/run-sweeps", web::post().to(run_sweeps)),
    );
}
