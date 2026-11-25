//! Who may call `/v1`: nobody is refused, but each address has a rate (token buckets kept in memory only
//! (IP addresses are never written to disk, as `/privacy` says)).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use axum::http::HeaderMap;

/// The client's address: behind Traefik, the last `X-Forwarded-For` entry is the one Traefik
/// added itself (the earlier ones come from the client and prove nothing).
pub fn client_ip(headers: &HeaderMap) -> String {
    headers
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .map(str::trim)
        .rfind(|ip| !ip.is_empty())
        .unwrap_or("inconnue")
        .to_owned()
}

/// Token buckets: `burst` requests at once, refilled at `per_hour`.
pub struct Limiter {
    burst: f64,
    per_second: f64,
    buckets: Mutex<HashMap<String, (f64, Instant)>>,
}

/// Above this many buckets, the full ones (idle callers) are forgotten.
const MAX_BUCKETS: usize = 10_000;

impl Limiter {
    pub fn new(burst: u32, per_hour: u32) -> Self {
        Self {
            burst: f64::from(burst),
            per_second: f64::from(per_hour) / 3600.0,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Takes one token from `who`'s bucket; `false` when it is empty.
    pub fn allow(&self, who: &str, now: Instant) -> bool {
        let mut buckets = self.buckets.lock().expect("limiter lock");
        if buckets.len() > MAX_BUCKETS {
            buckets.retain(|_, (tokens, at)| self.refill(*tokens, *at, now) < self.burst);
        }
        let (tokens, at) = buckets.entry(who.to_owned()).or_insert((self.burst, now));
        let available = self.refill(*tokens, *at, now);
        let allowed = available >= 1.0;
        *tokens = if allowed { available - 1.0 } else { available };
        *at = now;
        allowed
    }

    fn refill(&self, tokens: f64, at: Instant, now: Instant) -> f64 {
        let elapsed = now.saturating_duration_since(at).as_secs_f64();
        (tokens + elapsed * self.per_second).min(self.burst)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn the_last_forwarded_address_counts() {
        let mut headers = HeaderMap::new();
        assert_eq!(client_ip(&headers), "inconnue");
        headers.insert("x-forwarded-for", "6.6.6.6, 203.0.113.7".parse().unwrap());
        assert_eq!(client_ip(&headers), "203.0.113.7");
    }

    #[test]
    fn buckets_empty_then_refill() {
        let limiter = Limiter::new(2, 3600);
        let t = Instant::now();
        assert!(limiter.allow("a", t));
        assert!(limiter.allow("a", t));
        assert!(!limiter.allow("a", t));
        assert!(limiter.allow("b", t));
        // One token per second at 3600 / h.
        assert!(limiter.allow("a", t + Duration::from_secs(1)));
        assert!(!limiter.allow("a", t + Duration::from_secs(1)));
    }
}
