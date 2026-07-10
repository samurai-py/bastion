// SMTP/IMAP email channel via lettre + async-imap (CHAN-03).
//
// Security: the `From:` header of an inbound email is UNTRUSTED input (SMTP does not
// authenticate it — anyone can claim to be anyone). It is resolved to a trusted owner_id
// via OwnerMap (CR-03) exactly like every other channel's credential. Senders whose
// address is absent from the map are silently dropped (no reply, no session) — mirrors
// telegram.rs::handle_update. EMAIL_PASSWORD is never logged, even inside an IMAP/SMTP
// error's Display string (T-10-06-02).
//
// Pitfall 5 (10-RESEARCH.md): an IMAP IDLE session left open indefinitely is silently
// dropped by many servers around the ~29-minute mark — the receive loop re-issues IDLE
// every 25 minutes, well under that threshold, and falls back to a 60s poll loop if the
// mailbox does not advertise IDLE support at all.
use crate::agent::handle::AgentHandle;
use crate::channel::{Channel, OwnerMap};

/// Email channel (CHAN-03): SMTP send via `lettre`, IMAP receive via `async-imap`
/// (native `IDLE` with a polling fallback).
pub struct EmailChannel {
    pub(crate) imap_host: String,
    pub(crate) imap_port: u16,
    pub(crate) smtp_host: String,
    pub(crate) smtp_port: u16,
    pub(crate) username: String,
    pub(crate) password: String,
    pub(crate) default_persona: Option<String>,
    /// Trusted sender-address → owner_id map. Unmapped senders are silently
    /// dropped (CR-03).
    pub(crate) owner_map: OwnerMap,
}

impl EmailChannel {
    /// Build from `EMAIL_ADDRESS`/`EMAIL_PASSWORD`/`EMAIL_IMAP_HOST`/`EMAIL_SMTP_HOST`
    /// (required, fail loud) and `EMAIL_IMAP_PORT`/`EMAIL_SMTP_PORT` (optional,
    /// default 993/587). Never logs the password (T-10-06-02).
    pub fn from_env() -> anyhow::Result<Self> {
        let username =
            std::env::var("EMAIL_ADDRESS").map_err(|_| anyhow::anyhow!("EMAIL_ADDRESS not set"))?;
        let password = std::env::var("EMAIL_PASSWORD")
            .map_err(|_| anyhow::anyhow!("EMAIL_PASSWORD not set"))?;
        let imap_host = std::env::var("EMAIL_IMAP_HOST")
            .map_err(|_| anyhow::anyhow!("EMAIL_IMAP_HOST not set"))?;
        let imap_port = std::env::var("EMAIL_IMAP_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(993);
        let smtp_host = std::env::var("EMAIL_SMTP_HOST")
            .map_err(|_| anyhow::anyhow!("EMAIL_SMTP_HOST not set"))?;
        let smtp_port = std::env::var("EMAIL_SMTP_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(587);

        Ok(Self {
            imap_host,
            imap_port,
            smtp_host,
            smtp_port,
            username,
            password,
            default_persona: None,
            owner_map: OwnerMap::default(),
        })
    }

    /// Set the default persona for this channel (CHAN-04).
    pub fn with_default_persona(mut self, persona: impl Into<String>) -> Self {
        self.default_persona = Some(persona.into());
        self
    }

    /// Configure the trusted sender-address → owner_id map. Without this, all
    /// messages are dropped.
    pub fn with_owner_map(mut self, map: OwnerMap) -> Self {
        self.owner_map = map;
        self
    }
}

#[async_trait::async_trait]
impl Channel for EmailChannel {
    async fn run(self: Box<Self>, agent: AgentHandle) -> anyhow::Result<()> {
        email_loop(
            &self.imap_host,
            self.imap_port,
            &self.smtp_host,
            self.smtp_port,
            &self.username,
            &self.password,
            agent,
            &self.owner_map,
        )
        .await
    }

    fn default_persona(&self) -> Option<&str> {
        self.default_persona.as_deref()
    }
}

/// Parse raw RFC822 message bytes into `(from_address, body)`.
///
/// Extracts the bare email address out of either the `"Name <addr@x.com>"` or bare
/// `"addr@x.com"` form of the `From:` header, and the plain-text body (mailparse
/// decodes quoted-printable/base64 transfer encodings automatically). Bails if the
/// `From:` header is absent.
pub fn parse_email_message(raw: &[u8]) -> anyhow::Result<(String, String)> {
    use mailparse::MailHeaderMap;

    let mail = mailparse::parse_mail(raw)?;

    let from_header = mail
        .headers
        .get_first_value("From")
        .ok_or_else(|| anyhow::anyhow!("email has no From: header"))?;

    let from_address = extract_email_address(&from_header);
    let body = mail.get_body()?;

    Ok((from_address, body))
}

/// Extract the bare email address out of a `From:` header value, handling both the
/// `"Name <addr@x.com>"` display-name form and a bare `"addr@x.com"` form.
fn extract_email_address(header_value: &str) -> String {
    if let Some(start) = header_value.find('<') {
        if let Some(end) = header_value.find('>') {
            if end > start {
                return header_value[start + 1..end].trim().to_owned();
            }
        }
    }
    header_value.trim().to_owned()
}

/// Resolve a sender address to an owner via the OwnerMap and forward the message to
/// the shared AgentLoop. Returns Err whose message contains "not in owner map" when
/// the sender is unknown (CR-03: reject unknown senders — the `From:` header is
/// untrusted/spoofable input, mirrors telegram.rs's `handle_update`). Factored out
/// for unit testing without a live mailbox.
pub async fn handle_email_message(
    from_address: String,
    text: String,
    agent: &AgentHandle,
    owner_map: &OwnerMap,
) -> anyhow::Result<String> {
    let owner = owner_map
        .resolve(&from_address)
        .ok_or_else(|| {
            anyhow::anyhow!("email address {from_address} not in owner map — rejecting (CR-03)")
        })?
        .to_owned();
    agent.ask(text, owner).await
}

/// IMAP IDLE-with-poll-fallback receive loop + SMTP reply send.
///
/// Signature only in this commit (Task 2) — `run()` already delegates to it per the
/// plan's declared call shape. The real IDLE/poll loop + SMTP send body is implemented
/// in Task 3 of this plan.
// EmailChannel has more config fields (imap/smtp host+port, username, password) than any
// other channel in this phase — the 8-arg signature mirrors those fields 1:1 (per the
// plan's own call shape) rather than introducing a config struct for a single call site.
#[allow(clippy::too_many_arguments)]
async fn email_loop(
    _imap_host: &str,
    _imap_port: u16,
    _smtp_host: &str,
    _smtp_port: u16,
    _username: &str,
    _password: &str,
    _agent: AgentHandle,
    _owner_map: &OwnerMap,
) -> anyhow::Result<()> {
    anyhow::bail!("email_loop not yet implemented (Task 3 of 10-06-PLAN.md)")
}

// ─── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::handle;
    use tokio::sync::mpsc;

    /// Stub consumer: replies "echo:{text}".
    fn stub_consumer(mut rx: mpsc::Receiver<crate::agent::handle::AgentRequest>) {
        tokio::spawn(async move {
            while let Some(req) = rx.recv().await {
                let _ = req.reply.send(Ok(format!("echo:{}", req.text)));
            }
        });
    }

    const RAW_WITH_DISPLAY_NAME: &[u8] =
        b"From: Mario <mario@example.com>\r\nTo: bastion@example.com\r\nSubject: hi\r\n\r\nhello there\r\n";

    const RAW_BARE_ADDRESS: &[u8] =
        b"From: mario@example.com\r\nTo: bastion@example.com\r\nSubject: hi\r\n\r\nhello there\r\n";

    #[test]
    fn parse_email_message_extracts_address_from_display_name_form() {
        let (from, body) = parse_email_message(RAW_WITH_DISPLAY_NAME).unwrap();
        assert_eq!(from, "mario@example.com");
        assert!(body.contains("hello there"));
    }

    #[test]
    fn parse_email_message_extracts_bare_address() {
        let (from, body) = parse_email_message(RAW_BARE_ADDRESS).unwrap();
        assert_eq!(from, "mario@example.com");
        assert!(body.contains("hello there"));
    }

    #[tokio::test]
    async fn handle_email_message_routes_known_sender_to_agent() {
        let (h, rx) = handle::channel();
        stub_consumer(rx);
        let map = OwnerMap::from_pairs(&[("mario@example.com", "mario")]);

        let reply = handle_email_message("mario@example.com".into(), "ping".into(), &h, &map)
            .await
            .unwrap();
        assert_eq!(reply, "echo:ping");
    }

    #[tokio::test]
    async fn handle_email_message_rejects_unmapped_sender() {
        let (h, rx) = handle::channel();
        stub_consumer(rx);
        let map = OwnerMap::from_pairs(&[("mario@example.com", "mario")]);

        let result =
            handle_email_message("stranger@example.com".into(), "ping".into(), &h, &map).await;
        assert!(result.is_err(), "unmapped sender must be rejected");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("not in owner map"), "error message: {msg}");
    }

    #[tokio::test]
    async fn handle_email_message_empty_map_rejects_all() {
        let (h, rx) = handle::channel();
        stub_consumer(rx);
        let map = OwnerMap::default();

        let result =
            handle_email_message("mario@example.com".into(), "ping".into(), &h, &map).await;
        assert!(result.is_err());
    }
}
