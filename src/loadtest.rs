//! API load testing: N virtual users replay one prepared request for a duration (or a request
//! budget), sharing one keep-alive client. Samples stream to the UI, which aggregates them in `Stats`.

use crate::http::{self, Prepared};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct Config {
    pub users: usize,
    pub duration: Duration,
    /// 0 = unlimited (run for `duration`)
    pub max_requests: u64,
    pub ramp_up: Duration,
    pub think: Duration,
}

#[derive(Clone, Debug)]
pub struct Sample {
    /// Completion time since the test started.
    pub at: Duration,
    pub latency: Duration,
    pub status: Option<u16>,
    pub bytes: usize,
    pub error: Option<String>,
}

impl Sample {
    pub fn ok(&self) -> bool {
        self.error.is_none() && self.status.is_some_and(|s| s < 400)
    }
}

/// Runs the test on background threads; `tx` is dropped (closing the channel) when every user finished.
pub fn run(p: Prepared, cfg: Config, stop: Arc<AtomicBool>, tx: Sender<Sample>, notify: impl Fn() + Send + Sync + 'static) {
    let notify = Arc::new(notify);
    std::thread::spawn(move || {
        let client = match http::client_for(&p) {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(Sample { at: Duration::ZERO, latency: Duration::ZERO, status: None, bytes: 0, error: Some(e) });
                return;
            }
        };
        let p = Arc::new(p);
        let started = Instant::now();
        let issued = Arc::new(AtomicU64::new(0));
        let users = cfg.users.max(1);
        let handles: Vec<_> = (0..users)
            .map(|i| {
                let (client, p, stop, tx, issued, cfg, notify) = (client.clone(), p.clone(), stop.clone(), tx.clone(), issued.clone(), cfg.clone(), notify.clone());
                std::thread::spawn(move || {
                    // ramp-up: user i starts at i/users of the ramp-up window
                    let delay = cfg.ramp_up.mul_f64(i as f64 / users as f64);
                    while started.elapsed() < delay {
                        if stop.load(Ordering::Relaxed) {
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    while !stop.load(Ordering::Relaxed) && started.elapsed() < cfg.duration {
                        if cfg.max_requests > 0 && issued.fetch_add(1, Ordering::Relaxed) >= cfg.max_requests {
                            break;
                        }
                        let t = Instant::now();
                        let r = http::send_with(&client, &p);
                        let latency = t.elapsed();
                        let sample = match r {
                            Ok(resp) => Sample { at: started.elapsed(), latency, status: Some(resp.status), bytes: resp.body.len(), error: None },
                            Err(e) => Sample { at: started.elapsed(), latency, status: None, bytes: 0, error: Some(e) },
                        };
                        if tx.send(sample).is_err() {
                            return;
                        }
                        notify();
                        if !cfg.think.is_zero() {
                            std::thread::sleep(cfg.think);
                        }
                    }
                })
            })
            .collect();
        drop(tx);
        for h in handles {
            let _ = h.join();
        }
        notify();
    });
}

#[derive(Default, Clone, Debug)]
pub struct Second {
    pub ok: u64,
    pub failed: u64,
    pub latencies_us: Vec<u32>,
}

#[derive(Default, Debug)]
pub struct Stats {
    pub total: u64,
    pub failed: u64,
    pub bytes: u64,
    pub latencies_us: Vec<u32>,
    pub seconds: Vec<Second>,
    pub statuses: BTreeMap<u16, u64>,
    pub errors: BTreeMap<String, u64>,
    pub last_at: Duration,
    sorted: Vec<u32>,
    sorted_len: usize,
}

impl Stats {
    pub fn add(&mut self, s: &Sample) {
        self.total += 1;
        self.bytes += s.bytes as u64;
        let us = s.latency.as_micros().min(u32::MAX as u128) as u32;
        self.latencies_us.push(us);
        let sec = s.at.as_secs() as usize;
        if self.seconds.len() <= sec {
            self.seconds.resize(sec + 1, Second::default());
        }
        let bucket = &mut self.seconds[sec];
        bucket.latencies_us.push(us);
        if s.ok() {
            bucket.ok += 1;
        } else {
            self.failed += 1;
            bucket.failed += 1;
        }
        if let Some(code) = s.status {
            *self.statuses.entry(code).or_default() += 1;
        }
        if let Some(e) = &s.error {
            // group by message without request-specific detail
            let key: String = e.chars().take(140).collect();
            *self.errors.entry(key).or_default() += 1;
        }
        self.last_at = self.last_at.max(s.at);
    }

    /// Latency percentile in milliseconds (q in 0..=100), nearest-rank.
    pub fn percentile_ms(&mut self, q: f64) -> f64 {
        if self.sorted_len != self.latencies_us.len() {
            self.sorted = self.latencies_us.clone();
            self.sorted.sort_unstable();
            self.sorted_len = self.latencies_us.len();
        }
        percentile(&self.sorted, q)
    }

    pub fn avg_ms(&self) -> f64 {
        if self.latencies_us.is_empty() { 0.0 } else { self.latencies_us.iter().map(|&x| x as f64).sum::<f64>() / self.latencies_us.len() as f64 / 1000.0 }
    }

    pub fn max_ms(&self) -> f64 {
        self.latencies_us.iter().copied().max().unwrap_or(0) as f64 / 1000.0
    }

    pub fn rps(&self, elapsed: Duration) -> f64 {
        let secs = elapsed.as_secs_f64();
        if secs <= 0.0 { 0.0 } else { self.total as f64 / secs }
    }

    pub fn error_rate(&self) -> f64 {
        if self.total == 0 { 0.0 } else { self.failed as f64 * 100.0 / self.total as f64 }
    }
}

/// Nearest-rank percentile of a sorted microsecond list, in milliseconds.
pub fn percentile(sorted_us: &[u32], q: f64) -> f64 {
    if sorted_us.is_empty() {
        return 0.0;
    }
    let rank = ((q / 100.0) * sorted_us.len() as f64).ceil().max(1.0) as usize;
    sorted_us[rank.min(sorted_us.len()) - 1] as f64 / 1000.0
}

/// Per-second (avg, p95) latency in ms for the latency chart.
pub fn second_latency(s: &Second) -> Option<(f64, f64)> {
    if s.latencies_us.is_empty() {
        return None;
    }
    let mut v = s.latencies_us.clone();
    v.sort_unstable();
    let avg = v.iter().map(|&x| x as f64).sum::<f64>() / v.len() as f64 / 1000.0;
    Some((avg, percentile(&v, 95.0)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::BodyData;
    use std::io::{Read, Write};

    #[test]
    fn percentiles_and_stats() {
        let sorted: Vec<u32> = (1..=100).map(|ms| ms * 1000).collect();
        assert_eq!(percentile(&sorted, 50.0), 50.0);
        assert_eq!(percentile(&sorted, 95.0), 95.0);
        assert_eq!(percentile(&sorted, 99.0), 99.0);
        assert_eq!(percentile(&sorted, 100.0), 100.0);
        assert_eq!(percentile(&[], 50.0), 0.0);

        let mut st = Stats::default();
        let s = |at_ms: u64, lat_ms: u64, status: Option<u16>| Sample { at: Duration::from_millis(at_ms), latency: Duration::from_millis(lat_ms), status, bytes: 10, error: status.is_none().then(|| "connection refused".to_string()) };
        st.add(&s(100, 10, Some(200)));
        st.add(&s(900, 30, Some(500)));
        st.add(&s(1500, 20, None));
        assert_eq!((st.total, st.failed, st.bytes), (3, 2, 30));
        assert_eq!(st.seconds.len(), 2);
        assert_eq!((st.seconds[0].ok, st.seconds[0].failed), (1, 1));
        assert_eq!(st.statuses[&500], 1);
        assert_eq!(st.errors["connection refused"], 1);
        assert_eq!(st.avg_ms(), 20.0);
        assert_eq!(st.percentile_ms(50.0), 20.0);
        assert!((st.error_rate() - 66.666).abs() < 0.01);
        assert_eq!(second_latency(&st.seconds[0]), Some((20.0, 30.0)));
    }

    #[test]
    fn load_test_against_local_server() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                std::thread::spawn(move || {
                    let mut s = stream;
                    let mut buf = [0u8; 4096];
                    // keep-alive: answer every request on this connection
                    while let Ok(n) = s.read(&mut buf) {
                        if n == 0 {
                            break;
                        }
                        let _ = s.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok");
                    }
                });
            }
        });
        let p = Prepared { method: "GET".into(), url: format!("http://127.0.0.1:{port}/"), headers: vec![], body: BodyData::None, basic_auth: None, timeout_ms: 5000, follow_redirects: true, max_redirects: 5 };
        let cfg = Config { users: 4, duration: Duration::from_secs(10), max_requests: 200, ramp_up: Duration::ZERO, think: Duration::ZERO };
        let (tx, rx) = std::sync::mpsc::channel();
        run(p, cfg, Arc::new(AtomicBool::new(false)), tx, || {});
        let mut st = Stats::default();
        for s in rx.iter() {
            st.add(&s);
        }
        assert_eq!(st.total, 200, "request budget is exact");
        assert_eq!(st.failed, 0, "{:?}", st.errors);
        assert_eq!(st.statuses[&200], 200);
        assert!(st.percentile_ms(99.0) < 5000.0);
    }

    #[test]
    fn stop_flag_ends_the_run() {
        let p = Prepared { method: "GET".into(), url: "http://127.0.0.1:9/".into(), headers: vec![], body: BodyData::None, basic_auth: None, timeout_ms: 200, follow_redirects: true, max_redirects: 5 };
        let cfg = Config { users: 2, duration: Duration::from_secs(60), max_requests: 0, ramp_up: Duration::ZERO, think: Duration::from_millis(20) };
        let (tx, rx) = std::sync::mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        run(p, cfg, stop.clone(), tx, || {});
        std::thread::sleep(Duration::from_millis(300));
        stop.store(true, Ordering::Relaxed);
        let started = Instant::now();
        let samples: Vec<Sample> = rx.iter().collect();
        assert!(started.elapsed() < Duration::from_secs(3), "run stopped promptly");
        assert!(samples.iter().all(|s| !s.ok()), "port 9 refuses connections");
    }
}
