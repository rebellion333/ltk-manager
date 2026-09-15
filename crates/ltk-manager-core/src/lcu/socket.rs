//! The client's event socket.
//!
//! `wss://127.0.0.1:<port>` under the `wamp` subprotocol. A subscription is
//! `[5, "OnJsonApiEvent_<uri with / as _>"]`, and an event arrives as
//! `[8, "<name>", { "data": ..., "eventType": ..., "uri": ... }]`. Reads block:
//! an idle socket costs no wakeups, and [`LcuSocket::stopper`] is how another
//! thread ends a read that would otherwise wait for the next event.

use std::net::{Shutdown, TcpStream};

use serde::Deserialize;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

use super::client::authorization;
use super::lockfile::LeagueLockfile;

/// One delivered event.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LcuEventFrame {
    pub uri: String,
    /// `Create`, `Update` or `Delete`.
    pub event_type: String,
    #[serde(default)]
    pub data: serde_json::Value,
}

/// Ends a blocking read from another thread by closing the TCP side.
#[derive(Debug)]
pub struct SocketStopper(TcpStream);

impl SocketStopper {
    pub fn stop(&self) {
        let _ = self.0.shutdown(Shutdown::Both);
    }
}

/// A connected, authenticated socket.
pub struct LcuSocket {
    inner: WebSocket<MaybeTlsStream<TcpStream>>,
}

impl LcuSocket {
    /// Connect and complete the `wamp` handshake.
    pub fn connect(lockfile: &LeagueLockfile) -> Result<Self, Box<tungstenite::Error>> {
        let request = tungstenite::http::Request::builder()
            .uri(format!("wss://127.0.0.1:{}/", lockfile.port))
            .header("Host", format!("127.0.0.1:{}", lockfile.port))
            .header("Authorization", authorization(lockfile))
            .header("Sec-WebSocket-Protocol", "wamp")
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header(
                "Sec-WebSocket-Key",
                tungstenite::handshake::client::generate_key(),
            )
            .body(())
            .map_err(|e| Box::new(tungstenite::Error::Io(std::io::Error::other(e))))?;
        let tls = native_tls::TlsConnector::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(|e| Box::new(tungstenite::Error::Io(std::io::Error::other(e))))?;
        let stream = TcpStream::connect(("127.0.0.1", lockfile.port))
            .map_err(|e| Box::new(tungstenite::Error::Io(e)))?;
        let (inner, _response) = tungstenite::client_tls_with_config(
            request,
            stream,
            None,
            Some(tungstenite::Connector::NativeTls(tls)),
        )
        .map_err(|e| match e {
            tungstenite::HandshakeError::Failure(error) => Box::new(error),
            // A blocking stream never leaves a handshake half done.
            tungstenite::HandshakeError::Interrupted(_) => Box::new(tungstenite::Error::Io(
                std::io::Error::other("handshake interrupted"),
            )),
        })?;
        Ok(Self { inner })
    }

    /// Subscribe to one `OnJsonApiEvent_*` name.
    pub fn subscribe(&mut self, event_name: &str) -> Result<(), Box<tungstenite::Error>> {
        self.inner
            .send(Message::Text(format!(
                "[5,{}]",
                serde_json::json!(event_name)
            )))
            .map_err(Box::new)
    }

    /// A handle that can end the next read from another thread.
    pub fn stopper(&self) -> Option<SocketStopper> {
        match self.inner.get_ref() {
            MaybeTlsStream::NativeTls(tls) => tls.get_ref().try_clone().ok().map(SocketStopper),
            MaybeTlsStream::Plain(tcp) => tcp.try_clone().ok().map(SocketStopper),
            _ => None,
        }
    }

    /// Block until the next event, a control frame having been handled.
    ///
    /// `None` when the socket is gone, which is also what a stopper produces.
    pub fn next_event(&mut self) -> Option<LcuEventFrame> {
        loop {
            match self.inner.read() {
                Ok(Message::Text(text)) => {
                    if let Some(frame) = parse_frame(&text) {
                        return Some(frame);
                    }
                }
                Ok(Message::Close(_)) => return None,
                Ok(_) => {}
                Err(e) => {
                    tracing::debug!("LCU socket read ended: {e}");
                    return None;
                }
            }
        }
    }
}

/// `[8, name, frame]` to a frame. Anything else, including the empty text the
/// client sends on subscribe, is `None`.
pub fn parse_frame(text: &str) -> Option<LcuEventFrame> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let items = value.as_array()?;
    if items.first()?.as_u64()? != 8 {
        return None;
    }
    serde_json::from_value(items.get(2)?.clone()).ok()
}

/// The subscription name of a uri: `/lol-gameflow/v1/gameflow-phase` becomes
/// `OnJsonApiEvent_lol-gameflow_v1_gameflow-phase`.
pub fn event_name_for(uri: &str) -> String {
    format!("OnJsonApiEvent{}", uri.replace('/', "_"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_uri_names_its_subscription() {
        assert_eq!(
            event_name_for("/lol-gameflow/v1/gameflow-phase"),
            "OnJsonApiEvent_lol-gameflow_v1_gameflow-phase"
        );
        assert_eq!(
            event_name_for("/lol-champ-select/v1/session"),
            "OnJsonApiEvent_lol-champ-select_v1_session"
        );
    }

    #[test]
    fn an_event_frame_parses_and_the_rest_is_ignored() {
        let frame = parse_frame(
            r#"[8,"OnJsonApiEvent_lol-gameflow_v1_gameflow-phase",{"data":"ChampSelect","eventType":"Update","uri":"/lol-gameflow/v1/gameflow-phase"}]"#,
        )
        .unwrap();
        assert_eq!(frame.uri, "/lol-gameflow/v1/gameflow-phase");
        assert_eq!(frame.event_type, "Update");
        assert_eq!(frame.data, serde_json::json!("ChampSelect"));

        assert!(parse_frame("").is_none());
        assert!(parse_frame(r#"[5,"OnJsonApiEvent"]"#).is_none());
        assert!(parse_frame(r#"{"not":"an array"}"#).is_none());
    }
}
