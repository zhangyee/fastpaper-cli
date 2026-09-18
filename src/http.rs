//! The one place HTTP agents are built, so that no request can hang.
//!
//! ureq sets no timeouts at all by default, and every source used to call
//! `ureq::get` directly: a medRxiv lookup once sat five minutes on a server
//! that had accepted the connection and never answered.
//!
//! Every limit here is per request. The one that matters most is idleness --
//! no new bytes for 30 seconds -- which ureq's own settings cannot express:
//! they are all "this phase may take at most N in total", including the body.
//! A total is right for an API answer, which is small, and wrong for a 100 MB
//! PDF on a slow link that is still arriving. So the idle limit is enforced one
//! layer down, on every wait for network input, and a total is added only for
//! API calls.
//!
//! That layer uses ureq's `unversioned` transport API, which may change in a
//! minor release; Cargo.toml pins ureq to 3.3.x for that reason.

use std::sync::OnceLock;
use std::time::Duration;

use ureq::unversioned::resolver::DefaultResolver;
use ureq::unversioned::transport::{
    Buffers, ConnectionDetails, Connector, DefaultConnector, NextTimeout, Transport,
};

/// How long a request may take, per request.
pub struct Limits {
    /// Resolving the host and opening the connection.
    pub connect: Duration,
    /// Longest wait for the next bytes, headers or body alike.
    pub idle: Duration,
    /// The whole request, end to end. `None` for downloads.
    pub total: Option<Duration>,
}

/// API calls: search, get, cite. The slowest honest API measured here (osf)
/// takes 12-16 s for a search.
pub const API: Limits = Limits {
    connect: Duration::from_secs(10),
    idle: Duration::from_secs(30),
    total: Some(Duration::from_secs(30)),
};

/// File downloads: PDFs and figure archives, bounded by idleness alone.
pub const DOWNLOAD: Limits = Limits {
    connect: Duration::from_secs(10),
    idle: Duration::from_secs(30),
    total: None,
};

/// The shared agent for API calls.
pub fn api() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| agent(&API))
}

/// The shared agent for file downloads.
pub fn download() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| agent(&DOWNLOAD))
}

/// Build an agent held to `limits`.
pub fn agent(limits: &Limits) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .timeout_resolve(Some(limits.connect))
        .timeout_connect(Some(limits.connect))
        .timeout_global(limits.total)
        .build();
    let connector = DefaultConnector::new().chain(IdleLimit { idle: limits.idle });
    ureq::Agent::with_parts(config, connector, DefaultResolver::default())
}

/// Wraps whatever transport the default chain produced (plain TCP, TLS,
/// proxied) in [`IdleTransport`].
#[derive(Debug)]
struct IdleLimit {
    idle: Duration,
}

impl Connector<Box<dyn Transport>> for IdleLimit {
    type Out = IdleTransport;

    fn connect(
        &self,
        _: &ConnectionDetails,
        chained: Option<Box<dyn Transport>>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        Ok(chained.map(|inner| IdleTransport {
            inner,
            idle: self.idle,
        }))
    }
}

/// A transport whose every wait for input is cut to at most `idle`.
///
/// ureq hands each wait the time left before its own deadline; this caps it,
/// so the clock effectively restarts whenever bytes arrive. A wait that runs
/// out because of the cap is reported as idleness rather than as the phase
/// ureq was in.
#[derive(Debug)]
struct IdleTransport {
    inner: Box<dyn Transport>,
    idle: Duration,
}

impl Transport for IdleTransport {
    fn buffers(&mut self) -> &mut dyn Buffers {
        self.inner.buffers()
    }

    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        self.inner.transmit_output(amount, timeout)
    }

    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        let cap = self.idle.into();
        if timeout.after <= cap {
            return self.inner.await_input(timeout);
        }
        let capped = NextTimeout {
            after: cap,
            reason: timeout.reason,
        };
        match self.inner.await_input(capped) {
            Err(ureq::Error::Timeout(_)) => Err(ureq::Error::Io(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!(
                    "no data received for {} s",
                    self.idle.as_secs_f32().max(0.1)
                ),
            ))),
            other => other,
        }
    }

    fn is_open(&mut self) -> bool {
        self.inner.is_open()
    }

    fn is_tls(&self) -> bool {
        self.inner.is_tls()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::mpsc;
    use std::time::Instant;

    /// Serve one connection with `script`, returning the URL to reach it.
    fn serve(script: impl FnOnce(TcpStream) + Send + 'static) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 4096];
            let _ = stream.read(&mut request);
            script(stream);
        });
        url
    }

    /// Run `f`, failing the test rather than hanging it when `f` does.
    fn within<T: Send + 'static>(limit: Duration, f: impl FnOnce() -> T + Send + 'static) -> T {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        rx.recv_timeout(limit)
            .expect("request hung past the test's own limit")
    }

    fn fetch(limits: Limits, url: String) -> Result<Vec<u8>, String> {
        let agent = agent(&limits);
        let resp = agent.get(&url).call().map_err(|e| e.to_string())?;
        resp.into_body().read_to_vec().map_err(|e| e.to_string())
    }

    fn short(idle_ms: u64, total_ms: Option<u64>) -> Limits {
        Limits {
            connect: Duration::from_secs(2),
            idle: Duration::from_millis(idle_ms),
            total: total_ms.map(Duration::from_millis),
        }
    }

    const HEAD: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\n";

    // The medRxiv failure: the connection is up and the server never answers.
    #[test]
    fn a_server_that_never_answers_times_out() {
        let url = serve(|_stream| std::thread::sleep(Duration::from_secs(5)));
        let started = Instant::now();
        let err = within(Duration::from_secs(4), move || fetch(short(300, None), url)).unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(err.contains("no data"), "got: {}", err);
    }

    #[test]
    fn a_body_that_stops_arriving_times_out() {
        let url = serve(|mut stream| {
            stream.write_all(HEAD).unwrap();
            stream.write_all(b"12345").unwrap();
            std::thread::sleep(Duration::from_secs(5));
        });
        let err = within(Duration::from_secs(4), move || fetch(short(300, None), url)).unwrap_err();
        assert!(err.contains("no data"), "got: {}", err);
    }

    // A download that keeps moving is never cut off, however long it takes:
    // each byte arrives well inside the idle limit, the whole takes far longer.
    #[test]
    fn a_slow_but_steady_download_is_read_in_full() {
        let url = serve(|mut stream| {
            stream.write_all(HEAD).unwrap();
            for b in b"0123456789" {
                std::thread::sleep(Duration::from_millis(150));
                stream.write_all(&[*b]).unwrap();
            }
        });
        let started = Instant::now();
        let body = within(Duration::from_secs(5), move || fetch(short(400, None), url)).unwrap();
        assert_eq!(body, b"0123456789");
        assert!(started.elapsed() > Duration::from_millis(1200));
    }

    // API calls also bound the whole request: a trickle that never goes
    // idle still ends.
    #[test]
    fn a_total_limit_ends_a_trickle() {
        let url = serve(|mut stream| {
            stream.write_all(HEAD).unwrap();
            for b in b"0123456789" {
                std::thread::sleep(Duration::from_millis(150));
                let _ = stream.write_all(&[*b]);
            }
        });
        let started = Instant::now();
        assert!(
            within(Duration::from_secs(5), move || fetch(
                short(400, Some(500)),
                url
            ))
            .is_err()
        );
        assert!(started.elapsed() < Duration::from_millis(1200));
    }

    #[test]
    fn api_calls_are_bounded_in_total_and_downloads_only_by_idleness() {
        assert_eq!(API.total, Some(Duration::from_secs(30)));
        assert_eq!(DOWNLOAD.total, None);
        assert_eq!(API.idle, Duration::from_secs(30));
        assert_eq!(DOWNLOAD.idle, Duration::from_secs(30));
    }
}
