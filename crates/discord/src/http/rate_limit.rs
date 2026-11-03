//! Per-route and global rate-limit tracking.
//!
//! This module implements the client side of `docs/rate-limits.md`:
//!
//! * Buckets are identified by the `X-RateLimit-Bucket` header, with the quota
//!   tracked per bucket *and* per top-level resource (the route's *major
//!   parameter*), because Discord computes the limits independently for each
//!   channel, guild and webhook.
//! * `X-RateLimit-Remaining` and `X-RateLimit-Reset-After` say when a bucket
//!   may be used again; a request on an exhausted bucket waits until the
//!   bucket resets.
//! * A `429` carries `Retry-After` (or a `retry_after` body field) and a
//!   `global` flag; a global limit pauses every bot request, while a per-route
//!   limit pauses the bucket. Interaction endpoints are exempt from the global
//!   limit (`docs/rate-limits.md`, Global Rate Limit).
//!
//! All decisions are synchronous and take the current [`Instant`] as an
//! argument, so the state machine can be unit-tested without sleeping or
//! talking to Discord. [`RateLimiter`] wraps the state in a lock and performs
//! the sleeps.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use reqwest::header::HeaderMap;

use super::routes::Route;

/// Delay used when a `429` response carries neither `Retry-After` nor a
/// `retry_after` body field.
const DEFAULT_RETRY_AFTER: Duration = Duration::from_secs(1);

/// The rate-limit headers Discord sends with most responses.
///
/// Reference: `docs/rate-limits.md` (Header Format).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RateLimitHeaders {
    /// `X-RateLimit-Bucket`: the shared bucket these limits belong to.
    pub bucket: Option<String>,
    /// `X-RateLimit-Limit`: how many requests the bucket allows. Unused for
    /// decisions but kept for diagnostics.
    pub limit: Option<u32>,
    /// `X-RateLimit-Remaining`: how many requests are left in the bucket.
    pub remaining: Option<u32>,
    /// `X-RateLimit-Reset-After`: seconds until the bucket resets.
    pub reset_after: Option<Duration>,
    /// `X-RateLimit-Global`: present on a `429` when the global limit was hit.
    pub global: bool,
}

impl RateLimitHeaders {
    /// Parses the rate-limit headers out of a response.
    ///
    /// Missing or unparsable headers are ignored: the response is still a
    /// valid response, it just does not tell us anything about the bucket.
    #[must_use]
    pub fn from_headers(headers: &HeaderMap) -> Self {
        let remaining = header_str(headers, "x-ratelimit-remaining")
            .and_then(|value| value.trim().parse::<u32>().ok());
        let limit = header_str(headers, "x-ratelimit-limit")
            .and_then(|value| value.trim().parse::<u32>().ok());
        let reset_after = header_str(headers, "x-ratelimit-reset-after").and_then(parse_seconds);
        let bucket = header_str(headers, "x-ratelimit-bucket").map(str::to_owned);
        let global = header_str(headers, "x-ratelimit-global").is_some_and(parse_bool);

        Self {
            bucket,
            limit,
            remaining,
            reset_after,
            global,
        }
    }
}

/// What a `429 Too Many Requests` response asks the client to do.
///
/// Reference: `docs/rate-limits.md` (Exceeding A Rate Limit).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitInfo {
    /// How long to wait before retrying.
    pub retry_after: Duration,
    /// Whether the global per-bot limit (rather than a route limit) was hit.
    pub global: bool,
}

impl RateLimitInfo {
    /// Parses a `429` response following `docs/rate-limits.md`.
    ///
    /// The `Retry-After` header is preferred, then the JSON body's
    /// `retry_after` field; a response with neither waits
    /// `DEFAULT_RETRY_AFTER`. The global flag comes from the
    /// `X-RateLimit-Global` header or the body's `global` field.
    #[must_use]
    pub fn from_response(headers: &HeaderMap, body: &[u8]) -> Self {
        let json: Option<serde_json::Value> = serde_json::from_slice(body).ok();
        let retry_after = header_str(headers, "retry-after")
            .and_then(parse_seconds)
            .or_else(|| {
                json.as_ref()
                    .and_then(|value| value.get("retry_after"))
                    .and_then(serde_json::Value::as_f64)
                    .and_then(seconds_duration)
            })
            .unwrap_or(DEFAULT_RETRY_AFTER);

        let header_global = header_str(headers, "x-ratelimit-global").is_some_and(parse_bool);
        let body_global = json
            .as_ref()
            .and_then(|value| value.get("global"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        Self {
            retry_after,
            global: header_global || body_global,
        }
    }
}

/// The decision returned by [`LimiterState::try_acquire`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permit {
    /// A request may be sent now (and, when the bucket is known, a slot in it
    /// has been reserved).
    Now,
    /// The bucket is exhausted; wait before sending anything.
    Wait(Duration),
}

/// One tracked rate-limit bucket, for one major resource.
#[derive(Debug, Clone, Default)]
struct BucketState {
    /// The bucket's `X-RateLimit-Limit`, when known.
    limit: Option<u32>,
    /// The locally tracked remaining quota.
    remaining: Option<u32>,
    /// When the bucket resets (locally estimated from `Reset-After`).
    reset_at: Option<Instant>,
}

impl BucketState {
    /// Refills the bucket if its reset instant has passed.
    fn refresh(&mut self, now: Instant) {
        if self.reset_at.is_some_and(|reset_at| reset_at <= now) {
            self.remaining = self.limit;
            self.reset_at = None;
        }
    }

    /// Reserves a slot, or reports how long to wait first.
    fn try_acquire(&mut self, now: Instant) -> Permit {
        // A bucket whose reset instant has passed is full again.
        self.refresh(now);

        match self.remaining {
            // The bucket is empty. `refresh` guarantees that a known reset
            // instant is still in the future here, so waiting is correct.
            Some(0) => match self.reset_at {
                Some(reset_at) => Permit::Wait(reset_at.saturating_duration_since(now)),
                // The quota is exhausted but we do not know when it resets;
                // waiting forever would stall the bot, so send the request and
                // let the response correct the state.
                None => Permit::Now,
            },
            Some(remaining) => {
                self.remaining = Some(remaining.saturating_sub(1));
                Permit::Now
            }
            None => Permit::Now,
        }
    }
}

/// (bucket id, major parameter) — the unit Discord rate limits.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct BucketKey {
    bucket: String,
    major: Option<String>,
}

/// The mutable rate-limit state.
///
/// Every method takes the current [`Instant`], so tests can drive it with a
/// synthetic clock instead of sleeping.
#[derive(Debug, Default)]
pub struct LimiterState {
    /// Route key (`"POST /channels/{channel_id}/messages"`) -> bucket id.
    routes: HashMap<String, String>,
    /// Seen buckets.
    buckets: HashMap<BucketKey, BucketState>,
    /// When the global per-bot limit lifts, set by a global `429`.
    global_until: Option<Instant>,
}

impl LimiterState {
    /// Returns how long a bot request must wait for the global limit to lift,
    /// or `None` when it may proceed.
    ///
    /// Reference: `docs/rate-limits.md` (Global Rate Limit).
    #[must_use]
    pub fn global_wait(&self, now: Instant) -> Option<Duration> {
        let until = self.global_until?;
        let wait = until.saturating_duration_since(now);

        (!wait.is_zero()).then_some(wait)
    }

    /// Returns whether the state knows about any bucket for `route`.
    fn bucket_key(&self, route_key: &str, major: Option<&str>) -> Option<BucketKey> {
        let bucket = self.routes.get(route_key)?.clone();

        Some(BucketKey {
            bucket,
            major: major.map(str::to_owned),
        })
    }

    /// Reserves a slot in `route`'s bucket, or reports how long to wait.
    ///
    /// Until Discord has told us a bucket id for the route there is nothing to
    /// enforce, so the first request on a route always proceeds.
    pub fn try_acquire(&mut self, route_key: &str, major: Option<&str>, now: Instant) -> Permit {
        let Some(key) = self.bucket_key(route_key, major) else {
            return Permit::Now;
        };

        self.buckets.entry(key).or_default().try_acquire(now)
    }

    /// Stores the quota state a response reported, learning the bucket id for
    /// the route on the way.
    pub fn record_headers(
        &mut self,
        route_key: &str,
        major: Option<&str>,
        headers: &RateLimitHeaders,
        now: Instant,
    ) {
        if let Some(bucket) = &headers.bucket {
            self.routes.insert(route_key.to_owned(), bucket.clone());
        }

        let Some(key) = self.bucket_key(route_key, major) else {
            return;
        };

        let bucket = self.buckets.entry(key).or_default();

        if let Some(limit) = headers.limit {
            bucket.limit = Some(limit);
        }
        if let Some(remaining) = headers.remaining {
            bucket.remaining = Some(remaining);
        }
        if let Some(reset_after) = headers.reset_after {
            bucket.reset_at = now.checked_add(reset_after);
        }
    }

    /// Applies a `429` response: the bucket is empty until `retry_after`, and a
    /// global limit pauses every bot request.
    pub fn record_rate_limit(
        &mut self,
        route_key: &str,
        major: Option<&str>,
        info: &RateLimitInfo,
        now: Instant,
    ) {
        if info.global {
            self.global_until = now.checked_add(info.retry_after);
        }

        let key = if let Some(key) = self.bucket_key(route_key, major) {
            key
        } else {
            // A 429 normally repeats the bucket headers, but if this client has
            // never seen them we still must not retry in a tight loop: track a
            // synthetic bucket for the route so the retry waits.
            let bucket = format!("local:{route_key}");
            self.routes.insert(route_key.to_owned(), bucket.clone());

            BucketKey {
                bucket,
                major: major.map(str::to_owned),
            }
        };

        let bucket = self.buckets.entry(key).or_default();
        bucket.remaining = Some(0);
        bucket.reset_at = now.checked_add(info.retry_after);
    }

    /// Returns a reserved slot after a request failed without a response, so a
    /// transport error does not leak quota.
    pub fn release(&mut self, route_key: &str, major: Option<&str>) {
        let Some(key) = self.bucket_key(route_key, major) else {
            return;
        };

        if let Some(bucket) = self.buckets.get_mut(&key) {
            bucket.remaining = bucket
                .remaining
                .map(|remaining| remaining.saturating_add(1));
        }
    }
}

/// A [`LimiterState`] behind a lock, with the async waiting the client needs.
///
/// Cloning a `RateLimiter` shares the same state.
#[derive(Debug, Clone, Default)]
pub struct RateLimiter {
    state: Arc<Mutex<LimiterState>>,
}

impl RateLimiter {
    /// Creates an empty limiter.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Locks the state, recovering from a poisoned lock instead of panicking.
    fn lock(&self) -> MutexGuard<'_, LimiterState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Waits until the global per-bot limit has lifted.
    ///
    /// Reference: `docs/rate-limits.md` (Global Rate Limit). Only bot requests
    /// call this; interaction endpoints are exempt.
    pub async fn wait_for_global(&self) {
        loop {
            let wait = self.lock().global_wait(Instant::now());
            let Some(wait) = wait else {
                return;
            };

            tokio::time::sleep(wait).await;
        }
    }

    /// Waits until the route's bucket has room, then reserves a slot.
    ///
    /// Reference: `docs/rate-limits.md` (Header Format, Exceeding A Rate
    /// Limit).
    pub async fn wait_for_route(&self, route: &Route) {
        loop {
            let permit = self
                .lock()
                .try_acquire(route.key(), route.major(), Instant::now());

            match permit {
                Permit::Now => return,
                Permit::Wait(wait) => tokio::time::sleep(wait).await,
            }
        }
    }

    /// Stores the quota state a response reported.
    pub fn record_headers(&self, route: &Route, headers: &RateLimitHeaders) {
        self.lock()
            .record_headers(route.key(), route.major(), headers, Instant::now());
    }

    /// Applies a `429` response.
    pub fn record_rate_limit(&self, route: &Route, info: &RateLimitInfo) {
        self.lock()
            .record_rate_limit(route.key(), route.major(), info, Instant::now());
    }

    /// Returns a reserved slot after a request failed without a response.
    pub fn release(&self, route: &Route) {
        self.lock().release(route.key(), route.major());
    }
}

/// Parses an HTTP header value, ignoring headers that are not printable ASCII.
fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name)?.to_str().ok()
}

/// Parses a possibly-fractional number of seconds, ignoring negative or
/// non-finite values.
fn parse_seconds(value: &str) -> Option<Duration> {
    let seconds = value.trim().parse::<f64>().ok()?;

    seconds_duration(seconds)
}

/// Converts a number of seconds into a [`Duration`], ignoring values that are
/// negative, non-finite or too large.
fn seconds_duration(seconds: f64) -> Option<Duration> {
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }

    Duration::try_from_secs_f64(seconds).ok()
}

/// Parses the boolean-ish header values Discord sends (`true`, `1`).
fn parse_bool(value: &str) -> bool {
    matches!(value.trim().to_ascii_lowercase().as_str(), "true" | "1")
}

#[cfg(test)]
mod tests {
    use reqwest::header::HeaderValue;

    use super::*;
    use crate::id::{ApplicationId, ChannelId, EmojiId, GuildId, UserId};

    /// Builds a header map from the documented header examples.
    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(*name, HeaderValue::from_static(value));
        }
        map
    }

    /// The documented example headers from `docs/rate-limits.md` (Header
    /// Format).
    fn documented_headers() -> RateLimitHeaders {
        RateLimitHeaders::from_headers(&headers(&[
            ("X-RateLimit-Limit", "5"),
            ("X-RateLimit-Remaining", "0"),
            ("X-RateLimit-Reset", "1470173023"),
            ("X-RateLimit-Reset-After", "1"),
            ("X-RateLimit-Bucket", "abcd1234"),
        ]))
    }

    #[test]
    fn documented_headers_are_parsed() {
        let parsed = documented_headers();

        assert_eq!(parsed.bucket.as_deref(), Some("abcd1234"));
        assert_eq!(parsed.limit, Some(5));
        assert_eq!(parsed.remaining, Some(0));
        assert_eq!(parsed.reset_after, Some(Duration::from_secs(1)));
        assert!(!parsed.global);
    }

    #[test]
    fn fractional_reset_after_is_parsed() {
        let parsed = RateLimitHeaders::from_headers(&headers(&[
            ("X-RateLimit-Remaining", "9"),
            ("X-RateLimit-Reset-After", "64.57"),
        ]));

        assert_eq!(parsed.reset_after, Some(Duration::from_secs_f64(64.57)));
    }

    #[test]
    fn missing_headers_are_ignored() {
        assert_eq!(
            RateLimitHeaders::from_headers(&HeaderMap::new()),
            RateLimitHeaders::default()
        );

        let malformed = RateLimitHeaders::from_headers(&headers(&[
            ("X-RateLimit-Remaining", "not a number"),
            ("X-RateLimit-Reset-After", "-5"),
        ]));

        assert_eq!(malformed.remaining, None);
        assert_eq!(malformed.reset_after, None);
    }

    #[test]
    fn global_header_is_recognised() {
        let parsed = RateLimitHeaders::from_headers(&headers(&[
            ("X-RateLimit-Global", "true"),
            ("X-RateLimit-Scope", "global"),
        ]));

        assert!(parsed.global);
    }

    #[test]
    fn a_429_prefers_the_retry_after_header() {
        let info = RateLimitInfo::from_response(
            &headers(&[("Retry-After", "65"), ("X-RateLimit-Global", "true")]),
            br#"{"message":"You are being rate limited.","retry_after":64.57,"global":true}"#,
        );

        assert_eq!(info.retry_after, Duration::from_secs(65));
        assert!(info.global);
    }

    #[test]
    fn a_429_falls_back_to_the_body_retry_after() {
        let info = RateLimitInfo::from_response(
            &HeaderMap::new(),
            br#"{"message":"The resource is being rate limited.","retry_after":1336.57,"global":false}"#,
        );

        assert_eq!(info.retry_after, Duration::from_secs_f64(1336.57));
        assert!(!info.global);
    }

    #[test]
    fn a_429_without_a_delay_uses_the_default() {
        let info = RateLimitInfo::from_response(&HeaderMap::new(), b"not json");

        assert_eq!(info.retry_after, DEFAULT_RETRY_AFTER);
        assert!(!info.global);
    }

    #[test]
    fn body_global_flag_is_recognised() {
        let info = RateLimitInfo::from_response(
            &HeaderMap::new(),
            br#"{"message":"You are being rate limited.","retry_after":0.5,"global":true}"#,
        );

        assert!(info.global);
    }

    #[test]
    fn first_request_on_an_unknown_route_is_permitted() {
        let mut state = LimiterState::default();
        let now = Instant::now();

        assert_eq!(
            state.try_acquire(
                "POST /channels/{channel_id}/messages",
                Some("channel:1"),
                now
            ),
            Permit::Now
        );
    }

    #[test]
    fn exhausted_bucket_waits_until_reset() {
        let mut state = LimiterState::default();
        let route = "POST /channels/{channel_id}/messages";
        let t0 = Instant::now();

        state.record_headers(route, Some("channel:1"), &documented_headers(), t0);

        // The documented headers say remaining = 0 and reset in one second.
        assert_eq!(
            state.try_acquire(route, Some("channel:1"), t0),
            Permit::Wait(Duration::from_secs(1))
        );

        // After the reset instant the bucket refills to X-RateLimit-Limit.
        let t1 = t0 + Duration::from_secs(1);
        assert_eq!(state.try_acquire(route, Some("channel:1"), t1), Permit::Now);
    }

    #[test]
    fn remaining_quota_is_counted_down() {
        let mut state = LimiterState::default();
        let route = "POST /channels/{channel_id}/messages";
        let t0 = Instant::now();

        state.record_headers(
            route,
            Some("channel:1"),
            &RateLimitHeaders {
                bucket: Some("bucket".to_owned()),
                limit: Some(2),
                remaining: Some(2),
                reset_after: Some(Duration::from_secs(5)),
                global: false,
            },
            t0,
        );

        assert_eq!(state.try_acquire(route, Some("channel:1"), t0), Permit::Now);
        assert_eq!(state.try_acquire(route, Some("channel:1"), t0), Permit::Now);
        assert_eq!(
            state.try_acquire(route, Some("channel:1"), t0),
            Permit::Wait(Duration::from_secs(5))
        );
    }

    #[test]
    fn a_bucket_is_shared_between_routes_but_not_between_resources() {
        let mut state = LimiterState::default();
        let t0 = Instant::now();
        let limited = RateLimitHeaders {
            bucket: Some("shared".to_owned()),
            limit: Some(1),
            remaining: Some(0),
            reset_after: Some(Duration::from_secs(10)),
            global: false,
        };

        state.record_headers("PATCH /guilds/{guild_id}", Some("guild:1"), &limited, t0);
        state.record_headers(
            "DELETE /guilds/{guild_id}/members/{user_id}",
            Some("guild:1"),
            &limited,
            t0,
        );

        // Both routes share the bucket and the same guild, so both are empty.
        assert_eq!(
            state.try_acquire("PATCH /guilds/{guild_id}", Some("guild:1"), t0),
            Permit::Wait(Duration::from_secs(10))
        );
        assert_eq!(
            state.try_acquire(
                "DELETE /guilds/{guild_id}/members/{user_id}",
                Some("guild:1"),
                t0
            ),
            Permit::Wait(Duration::from_secs(10))
        );

        // A different guild has its own quota.
        state.record_headers(
            "PATCH /guilds/{guild_id}",
            Some("guild:2"),
            &RateLimitHeaders {
                bucket: Some("shared".to_owned()),
                limit: Some(1),
                remaining: Some(1),
                reset_after: None,
                global: false,
            },
            t0,
        );
        assert_eq!(
            state.try_acquire("PATCH /guilds/{guild_id}", Some("guild:2"), t0),
            Permit::Now
        );
    }

    #[test]
    fn a_429_empties_the_bucket_for_the_retry_after_window() {
        let mut state = LimiterState::default();
        let route = "PUT /guilds/{guild_id}/bans/{user_id}";
        let t0 = Instant::now();

        state.record_headers(
            route,
            Some("guild:1"),
            &RateLimitHeaders {
                bucket: Some("bans".to_owned()),
                limit: Some(5),
                remaining: Some(4),
                reset_after: Some(Duration::from_secs(1)),
                global: false,
            },
            t0,
        );
        state.record_rate_limit(
            route,
            Some("guild:1"),
            &RateLimitInfo {
                retry_after: Duration::from_secs(3),
                global: false,
            },
            t0,
        );

        assert_eq!(
            state.try_acquire(route, Some("guild:1"), t0),
            Permit::Wait(Duration::from_secs(3))
        );
        assert_eq!(
            state.try_acquire(route, Some("guild:1"), t0 + Duration::from_secs(3)),
            Permit::Now
        );
        assert_eq!(state.global_wait(t0), None);
    }

    #[test]
    fn a_global_429_pauses_all_bot_requests() {
        let mut state = LimiterState::default();
        let route = "POST /channels/{channel_id}/messages";
        let t0 = Instant::now();

        state.record_rate_limit(
            route,
            Some("channel:1"),
            &RateLimitInfo {
                retry_after: Duration::from_secs(65),
                global: true,
            },
            t0,
        );

        assert_eq!(state.global_wait(t0), Some(Duration::from_secs(65)));
        assert_eq!(
            state.global_wait(t0 + Duration::from_secs(65)),
            None,
            "the global limit has lifted after retry_after"
        );
    }

    #[test]
    fn transport_failure_releases_the_reserved_slot() {
        let mut state = LimiterState::default();
        let route = "POST /channels/{channel_id}/messages";
        let t0 = Instant::now();

        state.record_headers(
            route,
            Some("channel:1"),
            &RateLimitHeaders {
                bucket: Some("messages".to_owned()),
                limit: Some(5),
                remaining: Some(1),
                reset_after: Some(Duration::from_secs(5)),
                global: false,
            },
            t0,
        );

        assert_eq!(state.try_acquire(route, Some("channel:1"), t0), Permit::Now);
        assert_eq!(
            state.try_acquire(route, Some("channel:1"), t0),
            Permit::Wait(Duration::from_secs(5))
        );

        state.release(route, Some("channel:1"));
        assert_eq!(state.try_acquire(route, Some("channel:1"), t0), Permit::Now);
    }

    #[test]
    fn interaction_route_uses_the_route_key_without_a_major_parameter() {
        // Interaction routes have `major: None`; the limiter must still book
        // their buckets under the bare bucket id.
        let mut state = LimiterState::default();
        let route = "POST /interactions/{interaction_id}/{interaction_token}/callback";
        let t0 = Instant::now();

        state.record_headers(
            route,
            None,
            &RateLimitHeaders {
                bucket: Some("interactions".to_owned()),
                limit: Some(1),
                remaining: Some(0),
                reset_after: Some(Duration::from_secs(2)),
                global: false,
            },
            t0,
        );

        assert_eq!(
            state.try_acquire(route, None, t0),
            Permit::Wait(Duration::from_secs(2))
        );
    }

    #[test]
    fn a_429_on_an_unknown_route_still_blocks_the_retry() {
        let mut state = LimiterState::default();
        let route = "POST /channels/{channel_id}/messages";
        let t0 = Instant::now();

        // No `record_headers` call: the route has never reported a bucket.
        state.record_rate_limit(
            route,
            Some("channel:1"),
            &RateLimitInfo {
                retry_after: Duration::from_secs(2),
                global: false,
            },
            t0,
        );

        assert_eq!(
            state.try_acquire(route, Some("channel:1"), t0),
            Permit::Wait(Duration::from_secs(2))
        );
    }

    #[test]
    fn route_constructors_feed_the_limiter() {
        let guild = GuildId::new(1);
        let user = UserId::new(2);
        let route = Route::remove_guild_member(guild, user);

        assert_eq!(route.key(), "DELETE /guilds/{guild_id}/members/{user_id}");
        assert_eq!(route.major(), Some("guild:1"));
    }

    #[test]
    fn guild_command_buckets_are_independent_per_guild() {
        // docs/rate-limits.md (Per-route rate limits): "This means that an
        // endpoint with two different top-level resources may calculate limits
        // independently." An exhausted guild bucket must not stall command
        // registration in another guild.
        let mut state = LimiterState::default();
        let t0 = Instant::now();
        let limited = RateLimitHeaders {
            bucket: Some("commands".to_owned()),
            limit: Some(1),
            remaining: Some(0),
            reset_after: Some(Duration::from_secs(10)),
            global: false,
        };
        let first = Route::set_guild_commands(ApplicationId::new(42), GuildId::new(1));
        let second = Route::set_guild_commands(ApplicationId::new(42), GuildId::new(2));

        state.record_headers(first.key(), first.major(), &limited, t0);

        assert_eq!(
            state.try_acquire(first.key(), first.major(), t0),
            Permit::Wait(Duration::from_secs(10))
        );
        assert_eq!(
            state.try_acquire(second.key(), second.major(), t0),
            Permit::Now,
            "the application id must not collapse the per-guild buckets"
        );
    }

    #[test]
    fn application_emoji_route_has_no_major_parameter() {
        // docs/rate-limits.md: emoji routes do not follow the normal rate-limit
        // conventions, and applications are not a documented top-level
        // resource, so the route is tracked on its bucket alone.
        let route = Route::get_application_emoji(ApplicationId::new(42), EmojiId::new(99));

        assert_eq!(route.major(), None);
    }

    #[test]
    fn message_route_constructor_is_consistent() {
        let route = Route::create_message(ChannelId::new(3));

        assert_eq!(route.key(), "POST /channels/{channel_id}/messages");
        assert_eq!(route.major(), Some("channel:3"));
    }
}
