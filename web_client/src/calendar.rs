//! Calendar sync calls to `listing_api` (spec §3.6).
//!
//! Management calls carry the user's JWT in `X-Forwarded-Authorization`.
//! The export feed is fetched with the service identity only: no inbound
//! header is ever forwarded. Calls that sync feeds use a 25 s timeout, above
//! the API's 20 s handler deadline.

use crate::client::{TopcoatApiClient, UserCallError, decode_api, service_request};
use common::app_client::listing_api_audience;
use common::calendar::dto::{
    BlockedDateResponse, CalendarConflictsResponse, CalendarFeedResponse, CalendarOverview,
    CreateCalendarFeedResponse, FeedSyncResult, NewBlockedDateRequest, NewCalendarFeedRequest,
    UpdateCalendarFeedRequest, UpdateCalendarFeedResponse,
};
use reqwest::Method;
use std::time::Duration;
use uuid::Uuid;

/// Timeout of calls that sync feeds.
pub const SYNC_CALL_TIMEOUT: Duration = Duration::from_secs(25);

/// A fetched export feed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IcsFetch {
    /// The calendar and its strong ETag.
    Ok {
        /// The `.ics` body.
        body: String,
        /// The `ETag` header.
        etag: Option<String>,
    },
    /// Unchanged since the given ETag.
    NotModified {
        /// The `ETag` header.
        etag: Option<String>,
    },
    /// Unknown or deleted listing.
    NotFound,
}

fn map_send(e: reqwest::Error, timeout: Option<Duration>) -> UserCallError {
    match (e.is_timeout(), timeout) {
        (true, Some(t)) => UserCallError::TimedOut(t),
        _ => UserCallError::Transport(e.without_url()),
    }
}

async fn decode_empty(response: reqwest::Response) -> Result<(), UserCallError> {
    match response.status().is_success() {
        true => Ok(()),
        false => decode_api::<serde_json::Value>(response).await.map(|_| ()),
    }
}

impl TopcoatApiClient {
    fn calendar_url(&self, listing_id: Uuid, rest: &str) -> String {
        format!(
            "{}/api/v1/listings/{listing_id}/calendar{rest}",
            self.listing_api_url
        )
    }

    async fn calendar_call<B: serde::Serialize + ?Sized>(
        &self,
        method: Method,
        url: &str,
        jwt: &str,
        body: Option<&B>,
        timeout: Option<Duration>,
    ) -> Result<reqwest::Response, UserCallError> {
        let mut request = service_request(
            &self.client,
            method,
            url,
            &listing_api_audience(),
            Some(jwt),
        )
        .await?;
        if let Some(body) = body {
            request = request.json(body);
        }
        if let Some(t) = timeout {
            request = request.timeout(t);
        }
        request.send().await.map_err(|e| map_send(e, timeout))
    }

    /// A listing's imported calendars.
    pub async fn list_calendar_feeds(
        &self,
        listing_id: Uuid,
        jwt: &str,
    ) -> Result<Vec<CalendarFeedResponse>, UserCallError> {
        let url = self.calendar_url(listing_id, "/feeds");
        decode_api(
            self.calendar_call(Method::GET, &url, jwt, None::<&()>, None)
                .await?,
        )
        .await
    }

    /// Adds an imported calendar; the API syncs it straight away.
    pub async fn create_calendar_feed(
        &self,
        listing_id: Uuid,
        req: &NewCalendarFeedRequest,
        jwt: &str,
    ) -> Result<CreateCalendarFeedResponse, UserCallError> {
        let url = self.calendar_url(listing_id, "/feeds");
        let res = self
            .calendar_call(Method::POST, &url, jwt, Some(req), Some(SYNC_CALL_TIMEOUT))
            .await?;
        decode_api(res).await
    }

    /// Renames, re-points or switches an imported calendar.
    pub async fn update_calendar_feed(
        &self,
        listing_id: Uuid,
        feed_id: Uuid,
        req: &UpdateCalendarFeedRequest,
        jwt: &str,
    ) -> Result<UpdateCalendarFeedResponse, UserCallError> {
        let url = self.calendar_url(listing_id, &format!("/feeds/{feed_id}"));
        let res = self
            .calendar_call(Method::PATCH, &url, jwt, Some(req), Some(SYNC_CALL_TIMEOUT))
            .await?;
        decode_api(res).await
    }

    /// Removes an imported calendar.
    pub async fn delete_calendar_feed(
        &self,
        listing_id: Uuid,
        feed_id: Uuid,
        jwt: &str,
    ) -> Result<(), UserCallError> {
        let url = self.calendar_url(listing_id, &format!("/feeds/{feed_id}"));
        decode_empty(
            self.calendar_call(Method::DELETE, &url, jwt, None::<&()>, None)
                .await?,
        )
        .await
    }

    /// "Sync now": each feed's result, or a 429 rejection.
    pub async fn sync_calendar(
        &self,
        listing_id: Uuid,
        jwt: &str,
    ) -> Result<Vec<FeedSyncResult>, UserCallError> {
        let url = self.calendar_url(listing_id, "/sync");
        let res = self
            .calendar_call(
                Method::POST,
                &url,
                jwt,
                None::<&()>,
                Some(SYNC_CALL_TIMEOUT),
            )
            .await?;
        decode_api(res).await
    }

    /// Blocked dates from today to 24 months ahead, the longest range the
    /// API allows (so far-future manual blocks stay visible).
    pub async fn list_blocked_dates(
        &self,
        listing_id: Uuid,
        jwt: &str,
    ) -> Result<Vec<BlockedDateResponse>, UserCallError> {
        let from = common::reference::listing_today(chrono::Utc::now());
        let to = from
            .checked_add_months(chrono::Months::new(24))
            .unwrap_or(from);
        let url = self.calendar_url(listing_id, &format!("/blocks?from={from}&to={to}"));
        decode_api(
            self.calendar_call(Method::GET, &url, jwt, None::<&()>, None)
                .await?,
        )
        .await
    }

    /// Blocks nights by hand.
    pub async fn create_blocked_dates(
        &self,
        listing_id: Uuid,
        req: &NewBlockedDateRequest,
        jwt: &str,
    ) -> Result<BlockedDateResponse, UserCallError> {
        let url = self.calendar_url(listing_id, "/blocks");
        decode_api(
            self.calendar_call(Method::POST, &url, jwt, Some(req), None)
                .await?,
        )
        .await
    }

    /// Removes a manual block.
    pub async fn delete_blocked_dates(
        &self,
        listing_id: Uuid,
        block_id: Uuid,
        jwt: &str,
    ) -> Result<(), UserCallError> {
        let url = self.calendar_url(listing_id, &format!("/blocks/{block_id}"));
        decode_empty(
            self.calendar_call(Method::DELETE, &url, jwt, None::<&()>, None)
                .await?,
        )
        .await
    }

    /// Open conflicts and one page of acknowledged history.
    pub async fn list_calendar_conflicts(
        &self,
        listing_id: Uuid,
        page: i64,
        jwt: &str,
    ) -> Result<CalendarConflictsResponse, UserCallError> {
        let url = self.calendar_url(listing_id, &format!("/conflicts?page={}", page.max(1)));
        decode_api(
            self.calendar_call(Method::GET, &url, jwt, None::<&()>, None)
                .await?,
        )
        .await
    }

    /// Acknowledges a conflict.
    pub async fn acknowledge_calendar_conflict(
        &self,
        listing_id: Uuid,
        conflict_id: Uuid,
        jwt: &str,
    ) -> Result<(), UserCallError> {
        let url = self.calendar_url(listing_id, &format!("/conflicts/{conflict_id}/acknowledge"));
        decode_empty(
            self.calendar_call(Method::POST, &url, jwt, None::<&()>, None)
                .await?,
        )
        .await
    }

    /// Calendar problems across the user's listings.
    pub async fn calendar_overview(&self, jwt: &str) -> Result<CalendarOverview, UserCallError> {
        let url = format!("{}/api/v1/calendar/overview", self.listing_api_url);
        let request = service_request(
            &self.client,
            Method::GET,
            &url,
            &listing_api_audience(),
            Some(jwt),
        )
        .await?;
        decode_api(request.send().await.map_err(|e| map_send(e, None))?).await
    }

    /// The public export feed of `listing_id`, with the service identity
    /// only. `if_none_match` is the caller's cached ETag, never a header
    /// copied from an inbound request.
    pub async fn get_calendar_ics(
        &self,
        listing_id: Uuid,
        if_none_match: Option<&str>,
    ) -> Result<IcsFetch, UserCallError> {
        let url = format!(
            "{}/api/v1/listings/{listing_id}/calendar.ics",
            self.listing_api_url
        );
        let mut request = self
            .client
            .get_request(&url, &listing_api_audience())
            .await
            .map_err(UserCallError::Identity)?;
        if let Some(tag) = if_none_match {
            request = request.header(reqwest::header::IF_NONE_MATCH, tag);
        }
        let res = request.send().await.map_err(|e| map_send(e, None))?;
        let etag = res
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        match res.status().as_u16() {
            200 => Ok(IcsFetch::Ok {
                body: res.text().await.map_err(|e| map_send(e, None))?,
                etag,
            }),
            304 => Ok(IcsFetch::NotModified { etag }),
            404 => Ok(IcsFetch::NotFound),
            other => Err(UserCallError::Status(other)),
        }
    }
}
