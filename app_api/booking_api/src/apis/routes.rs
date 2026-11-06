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
use super::currencies;
use super::documents::{
    get_booking_document_by_number, get_booking_documents, get_cancellation_quote,
};
use super::exchange_rates;
use super::ledger::{
    export_host_ledger_csv, get_host_ledger_entries, get_host_ledger_summary,
    update_admin_payout_status,
};
use super::messages::{get_booking_messages, mark_booking_messages_read, send_booking_message};
use super::reservation_changes::{
    approve_reservation_change_request, cancel_reservation_change_request,
    create_reservation_change_request, decline_reservation_change_request,
    list_reservation_change_requests, quote_reservation_change,
};
use super::{
    availability, booking_requests, booking_updates, bookings, conversations, cron, documents,
    enquiries, inbox, messages, promo_codes, reservation_changes, stay_pricing,
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
            exchange_rates::current_rates,
            exchange_rates::coverage,
            exchange_rates::history,
            exchange_rates::publish,
            exchange_rates::delete_scheduled,
            exchange_rates::retire,
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
            currencies::list_active_currencies,
            currencies::admin_list_currencies,
            currencies::create_currency,
            currencies::update_currency,
            stay_pricing::public_listing_quote,
            stay_pricing::signed_in_quote,
            promo_codes::list_promo_codes,
            promo_codes::get_promo_code,
            promo_codes::create_promo_code,
            promo_codes::deactivate_promo_code,
            booking_requests::create_booking_request,
            booking_requests::get_booking_request,
            booking_requests::my_booking_requests,
            booking_requests::accept_booking_request,
            booking_requests::decline_booking_request,
            booking_requests::withdraw_booking_request,
            booking_requests::checkout_booking_request,
            conversations::get_conversation_messages,
            conversations::post_conversation_message,
            conversations::mark_conversation_read,
            enquiries::create_enquiry,
            enquiries::get_enquiry,
            enquiries::close_enquiry,
            enquiries::quote_enquiry,
            booking_requests::preview_quote,
            booking_requests::send_quote,
            inbox::get_inbox,
        ),
        components(
            schemas(
                NewBookingRequest,
                UpdatedBookingRequest,
                common::booking_request::CreateBookingRequestBody,
                common::booking_request::BookingRequestResponse,
                common::booking_request::BookingRequestList,
                common::booking_request::DeclineRequestBody,
                common::booking_request::CheckoutRequestBody,
                common::booking_request::CreateEnquiryBody,
                common::booking_request::QuoteRequestBody,
                common::booking_request::InboxItem,
                common::booking_request::InboxResponse,
                common::booking_request::HostResponseMetrics,
                common::booking_request::EnquiryResponse,
                common::booking_request::EnquiryStatus,
                common::booking_request::EnquiryClosedReason,
                common::booking_request::PriceBreakdown,
                common::booking_request::QuoteSummary,
                common::booking_request::RequestStatus,
                common::booking_request::RequestOrigin,
                common::booking_request::DeclineReason,
                common::booking_request::BookingMode,
                common::models::TransferBookingRequest,
                AvailabilityResponse,
                common::availability::UnavailableRange,
                common::availability::UnavailableRangesResponse,
                common::exchange_rate::ExchangeRate,
                common::exchange_rate::ExchangeRatesResponse,
                common::exchange_rate::PublishRateRequest,
                common::exchange_rate::RateVersion,
                common::exchange_rate::RateState,
                common::exchange_rate::PairHistoryResponse,
                common::exchange_rate::RateCoverageGap,
                common::currency::Currency,
                common::currency::CurrenciesResponse,
                common::currency::CreateCurrencyRequest,
                common::currency::UpdateCurrencyRequest,
                common::currency::ListingRef,
                common::currency::CurrencyInUse,
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
                common::models::StayQuoteRequest,
                common::models::StayQuoteResponse,
                common::models::StayQuotePromo,
                common::models::StayQuoteRefusal,
                common::promo::PromoOutcome,
                common::promo::PromoDiscountType,
                common::promo::PromoScope,
                common::promo::CreatePromoCodeRequest,
                common::promo::PromoCodeResponse,
                common::promo::PromoCodesResponse,
            )
        ),
        tags(
            (name = "bookings", description = "Booking management endpoints"),
            (name = "currencies", description = "Currency reference data"),
            (name = "exchange-rates", description = "Exchange-rate administration"),
            (name = "promo-codes", description = "Promo code administration"),
            (name = "booking-requests", description = "Enquiries and requests to book")
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
        web::resource("/api/v1/bookings/listing/{id}/quote")
            .route(web::get().to(stay_pricing::public_listing_quote)),
    );
    cfg.service(
        web::resource("/api/v1/bookings/listing/{id}/unavailable-ranges")
            .route(web::get().to(get_unavailable_ranges)),
    );

    cfg.service(
        web::scope("/api/v1/conversations")
            .wrap(from_fn(require_user))
            .wrap(from_fn(content_negotiation_middleware))
            .route(
                "/{id}/messages",
                web::get().to(conversations::get_conversation_messages),
            )
            .route(
                "/{id}/messages",
                web::post().to(conversations::post_conversation_message),
            )
            .route(
                "/{id}/messages/read",
                web::patch().to(conversations::mark_conversation_read),
            ),
    );

    cfg.service(
        web::scope("/api/v1/enquiries")
            .wrap(from_fn(require_user))
            .wrap(from_fn(content_negotiation_middleware))
            .route("", web::post().to(enquiries::create_enquiry))
            .route("/{id}", web::get().to(enquiries::get_enquiry))
            .route("/{id}/close", web::post().to(enquiries::close_enquiry))
            .route("/{id}/quote", web::post().to(enquiries::quote_enquiry)),
    );

    cfg.service(
        web::resource("/api/v1/inbox")
            .wrap(from_fn(content_negotiation_middleware))
            .wrap(from_fn(require_user))
            .route(web::get().to(inbox::get_inbox)),
    );

    // Requests to book: signed-in users only; handlers check the caller is
    // the request's guest, the listing's host or an admin.
    cfg.service(
        web::scope("/api/v1/booking-requests")
            .wrap(from_fn(require_user))
            .wrap(from_fn(content_negotiation_middleware))
            .route("", web::post().to(booking_requests::create_booking_request))
            .route(
                "/mine",
                web::get().to(booking_requests::my_booking_requests),
            )
            .route(
                "/{id}",
                web::get().to(booking_requests::get_booking_request),
            )
            .route(
                "/{id}/accept",
                web::post().to(booking_requests::accept_booking_request),
            )
            .route(
                "/{id}/decline",
                web::post().to(booking_requests::decline_booking_request),
            )
            .route(
                "/{id}/withdraw",
                web::post().to(booking_requests::withdraw_booking_request),
            )
            .route(
                "/{id}/checkout",
                web::post().to(booking_requests::checkout_booking_request),
            )
            .route(
                "/{id}/quote-preview",
                web::post().to(booking_requests::preview_quote),
            )
            .route("/{id}/quote", web::post().to(booking_requests::send_quote)),
    );

    // Everything else needs a signed-in user; handlers then check that the
    // user may act on the specific booking.
    cfg.service(
        web::scope("/api/v1/bookings")
            .wrap(from_fn(require_user))
            .route("/quote", web::post().to(stay_pricing::signed_in_quote))
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

    // Public: active currencies for selectors (no auth).
    cfg.service(
        web::resource("/api/v1/currencies")
            .route(web::get().to(currencies::list_active_currencies)),
    );
    cfg.service(
        web::scope("/api/v1/admin/currencies")
            .wrap(from_fn(require_user))
            .route("", web::get().to(currencies::admin_list_currencies))
            .route("", web::post().to(currencies::create_currency))
            .route("/{code}", web::patch().to(currencies::update_currency)),
    );

    cfg.service(
        web::scope("/api/v1/admin/exchange-rates")
            .wrap(from_fn(require_user))
            .route("", web::get().to(exchange_rates::current_rates))
            .route("", web::post().to(exchange_rates::publish))
            .route("/coverage", web::get().to(exchange_rates::coverage))
            .route("/{id}", web::delete().to(exchange_rates::delete_scheduled))
            .route(
                "/{base}/{target}/history",
                web::get().to(exchange_rates::history),
            )
            .route(
                "/{base}/{target}/retire",
                web::post().to(exchange_rates::retire),
            ),
    );

    cfg.service(
        web::scope("/api/v1/admin/promo-codes")
            .wrap(from_fn(require_user))
            .route("", web::get().to(promo_codes::list_promo_codes))
            .route("", web::post().to(promo_codes::create_promo_code))
            .route("/{id}", web::get().to(promo_codes::get_promo_code))
            .route(
                "/{id}/deactivate",
                web::post().to(promo_codes::deactivate_promo_code),
            ),
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
