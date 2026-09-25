#[cfg(test)]
mod tests {
    use crate::apis::{PubSubMessage, PubSubPushEnvelope, configure_routes_with_provider};
    use crate::config::WorkerConfig;
    use crate::processor::{load_template, process_email_event, render_email_body};
    use crate::provider::MockEmailProvider;
    use actix_web::{App, test, web};
    use base64::Engine;
    use common::email::EmailNotificationEvent;
    use db_core::models::DbEmailStatus;
    use sqlx_db_tester::TestPg;
    use std::env;
    use std::path::Path;
    use std::sync::Arc;

    async fn setup_test_db() -> TestPg {
        dotenvy::dotenv().ok();
        let db_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:password@localhost:5432/our_places".to_string()
        });
        TestPg::new(db_url, Path::new("../../db_core/migrations"))
    }

    #[tokio::test]
    async fn test_render_email_body_templates() {
        let otp_payload = serde_json::json!({ "code": "123456" });
        let otp_html = render_email_body("user_verification_otp", &otp_payload);
        assert!(otp_html.contains("123456"));
        assert!(otp_html.contains("Welcome to Our Places!"));

        let reset_payload = serde_json::json!({ "token": "rst_999" });
        let reset_html = render_email_body("password_reset_otp", &reset_payload);
        assert!(reset_html.contains("rst_999"));

        let booking_payload = serde_json::json!({
            "confirmation_code": "CONF123",
            "dates": "Oct 1 - Oct 5",
            "total_price": "$1,200.00"
        });
        let booking_html = render_email_body("booking_confirmation", &booking_payload);
        assert!(booking_html.contains("CONF123"));
        assert!(booking_html.contains("$1,200.00"));

        let message_payload = serde_json::json!({
            "sender_name": "Alice",
            "message_text": "What is the check-in time?"
        });
        let message_html = render_email_body("guest_host_message_notification", &message_payload);
        assert!(message_html.contains("Alice"));
        assert!(message_html.contains("What is the check-in time?"));
    }

    #[tokio::test]
    async fn test_load_template_from_external_file() {
        let content = load_template("user_verification_otp.html");
        assert!(content.contains("Welcome to Our Places!"));
        assert!(content.contains("{{code}}") || content.contains("{code}"));
    }

    #[tokio::test]
    async fn test_processor_successful_dispatch() {
        let test_db = setup_test_db().await;
        let pool = test_db.get_pool().await;

        let email = db_core::email_outbox::insert_email_outbox(
            &pool,
            "guest@example.com",
            "Your Booking",
            "booking_confirmation",
            &serde_json::json!({ "confirmation_code": "BK-100" }),
            3,
        )
        .await
        .expect("Failed to insert email");

        let provider = MockEmailProvider::new();
        let res = process_email_event(&pool, &provider, email.id, 3).await;
        assert!(res.is_ok());

        let sent = db_core::email_outbox::get_email_outbox_by_id(&pool, email.id)
            .await
            .expect("Query failed")
            .expect("Not found");
        assert_eq!(sent.status, DbEmailStatus::Sent);
        assert!(sent.sent_at.is_some());
        assert_eq!(sent.attempts, 1);

        let sent_emails = provider.get_sent_emails().await;
        assert_eq!(sent_emails.len(), 1);
        assert_eq!(sent_emails[0].0, "guest@example.com");
        assert_eq!(sent_emails[0].1, "Your Booking");
    }

    #[tokio::test]
    async fn test_processor_retry_until_success() {
        let test_db = setup_test_db().await;
        let pool = test_db.get_pool().await;

        let email = db_core::email_outbox::insert_email_outbox(
            &pool,
            "guest_retry@example.com",
            "OTP Code",
            "user_verification_otp",
            &serde_json::json!({ "code": "888888" }),
            3,
        )
        .await
        .expect("Failed to insert email");

        // Fails on attempt 1, succeeds on attempt 2
        let provider = MockEmailProvider::with_failures(1);
        let res = process_email_event(&pool, &provider, email.id, 3).await;
        assert!(res.is_ok());

        let sent = db_core::email_outbox::get_email_outbox_by_id(&pool, email.id)
            .await
            .expect("Query failed")
            .expect("Not found");
        assert_eq!(sent.status, DbEmailStatus::Sent);
        assert!(sent.sent_at.is_some());
        assert_eq!(sent.attempts, 2);
    }

    #[tokio::test]
    async fn test_processor_retries_exhausted_permanently_fails() {
        let test_db = setup_test_db().await;
        let pool = test_db.get_pool().await;

        let email = db_core::email_outbox::insert_email_outbox(
            &pool,
            "always_fails@example.com",
            "Notification",
            "user_verification_otp",
            &serde_json::json!({ "code": "000000" }),
            3,
        )
        .await
        .expect("Failed to insert email");

        // Fails 10 times, max_retries is 3
        let provider = MockEmailProvider::with_failures(10);
        let res = process_email_event(&pool, &provider, email.id, 3).await;
        assert!(res.is_ok());

        let failed = db_core::email_outbox::get_email_outbox_by_id(&pool, email.id)
            .await
            .expect("Query failed")
            .expect("Not found");
        assert_eq!(failed.status, DbEmailStatus::Failed);
        assert_eq!(failed.attempts, 3);
        assert!(failed.last_error.is_some());
    }

    #[tokio::test]
    async fn test_processor_idempotency_duplicate_event() {
        let test_db = setup_test_db().await;
        let pool = test_db.get_pool().await;

        let email = db_core::email_outbox::insert_email_outbox(
            &pool,
            "idempotent@example.com",
            "Single Send Only",
            "user_verification_otp",
            &serde_json::json!({ "code": "111111" }),
            3,
        )
        .await
        .expect("Failed to insert email");

        let provider = MockEmailProvider::new();
        // 1st dispatch
        let res1 = process_email_event(&pool, &provider, email.id, 3).await;
        assert!(res1.is_ok());
        assert_eq!(provider.get_sent_emails().await.len(), 1);

        // 2nd dispatch (duplicate delivery simulation)
        let res2 = process_email_event(&pool, &provider, email.id, 3).await;
        assert!(res2.is_ok());
        // Must NOT have dispatched a second time
        assert_eq!(provider.get_sent_emails().await.len(), 1);
    }

    #[actix_web::test]
    async fn test_pubsub_endpoint_authentication_and_dispatch() {
        let test_db = setup_test_db().await;
        let pool = test_db.get_pool().await;

        let email = db_core::email_outbox::insert_email_outbox(
            &pool,
            "auth_test@example.com",
            "Secret Test",
            "user_verification_otp",
            &serde_json::json!({ "code": "999999" }),
            3,
        )
        .await
        .expect("Failed to insert email");

        let secret = "test_pubsub_secret_token";
        let config = WorkerConfig {
            pubsub_secret_token: Some(secret.to_string()),
            max_retries: 3,
            use_mock_provider: true,
        };
        let mock_provider = Arc::new(MockEmailProvider::new());

        let app = test::init_service(App::new().app_data(web::Data::new(pool.clone())).configure(
            |cfg| {
                configure_routes_with_provider(cfg, mock_provider.clone(), config.clone());
            },
        ))
        .await;

        let event = EmailNotificationEvent { email_id: email.id };
        let event_json = serde_json::to_vec(&event).unwrap();
        let b64_data = base64::engine::general_purpose::STANDARD.encode(event_json);

        let push_payload = PubSubPushEnvelope {
            message: PubSubMessage {
                data: b64_data,
                _message_id: Some("msg-123".into()),
                _publish_time: None,
            },
        };

        // 1. Without Token -> 401 Unauthorized
        let req_unauth = test::TestRequest::post()
            .uri("/pubsub/email-events")
            .set_json(&push_payload)
            .to_request();
        let resp_unauth = test::call_service(&app, req_unauth).await;
        assert_eq!(resp_unauth.status(), 401);

        // 2. With Correct Token -> 200 OK
        let req_auth = test::TestRequest::post()
            .uri("/pubsub/email-events")
            .insert_header(("X-PubSub-Secret-Token", secret))
            .set_json(&push_payload)
            .to_request();
        let resp_auth = test::call_service(&app, req_auth).await;
        assert_eq!(resp_auth.status(), 200);

        // Verify status in DB is now Sent
        let updated = db_core::email_outbox::get_email_outbox_by_id(&pool, email.id)
            .await
            .expect("Query failed")
            .expect("Not found");
        assert_eq!(updated.status, DbEmailStatus::Sent);
    }
}
