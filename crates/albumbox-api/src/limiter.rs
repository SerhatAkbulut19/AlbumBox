//! Failed-login throttling per client IP.

use std::{
    collections::HashMap,
    net::IpAddr,
    sync::Mutex,
    time::{Duration, Instant},
};

const MAX_FAILURES: u32 = 10;
const WINDOW: Duration = Duration::from_secs(10 * 60);

#[derive(Default)]
pub struct LoginLimiter {
    failures: Mutex<HashMap<IpAddr, (u32, Instant)>>,
}

impl LoginLimiter {
    pub fn is_blocked(&self, ip: Option<IpAddr>) -> bool {
        let Some(ip) = ip else { return false };
        let map = self.failures.lock().unwrap();
        matches!(map.get(&ip), Some((count, since)) if *count >= MAX_FAILURES && since.elapsed() < WINDOW)
    }

    pub fn record_failure(&self, ip: Option<IpAddr>) {
        let Some(ip) = ip else { return };
        let mut map = self.failures.lock().unwrap();
        map.retain(|_, (_, since)| since.elapsed() < WINDOW);
        let entry = map.entry(ip).or_insert((0, Instant::now()));
        entry.0 += 1;
    }

    pub fn record_success(&self, ip: Option<IpAddr>) {
        if let Some(ip) = ip {
            self.failures.lock().unwrap().remove(&ip);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_after_max_failures() {
        let l = LoginLimiter::default();
        let ip = Some("192.168.1.20".parse().unwrap());
        for _ in 0..MAX_FAILURES {
            assert!(!l.is_blocked(ip));
            l.record_failure(ip);
        }
        assert!(l.is_blocked(ip));
        l.record_success(ip);
        assert!(!l.is_blocked(ip));
    }
}
