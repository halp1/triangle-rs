use std::{collections::VecDeque, sync::Arc};

use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use native_tls::TlsConnector;
use serde_json::{Value, json};
use tokio::sync::{Mutex, RwLock, mpsc};
use tokio_tungstenite::{
  Connector, connect_async_tls_with_config,
  tungstenite::{Message, http::Request},
};

use crate::{
  error::{Result, TriangleError},
  types::game::Handling,
  utils::{Api, EventEmitter, api::SpoolResult},
};

/// Internal mutable state managed by the background task.
struct Inner {
  received_id: u32,
  sent_id: u32,
  sent_queue: VecDeque<(u32, String)>,
  received_queue: VecDeque<(u32, Value)>,
  session: Session,
}

#[derive(Default)]
struct Session {
  ribbon_id: Option<String>,
  token_id: Option<String>,
}

#[allow(clippy::large_enum_variant)]
enum OutMsg {
  Send(String, Value),
  Die,
}

/// WebSocket connection to TETR.IO ribbon.
///
/// Uses JSON transport only (binary/amber excluded per project scope).
pub struct Ribbon {
  /// All ribbon events are broadcast here.
  pub emitter: Arc<EventEmitter>,
  send_tx: mpsc::UnboundedSender<OutMsg>,
  /// Reason from the last disconnect / kick.
  pub last_disconnect_reason: Arc<RwLock<String>>,
}

impl Ribbon {
  /// Connect to TETR.IO and run the ribbon handshake.
  ///
  /// Resolves when the ribbon task has been spawned, **not** when
  /// `server.authorize` has completed — subscribe to `client.ready` /
  /// `client.fail` on `emitter` afterwards.
  pub async fn connect(
    token: String,
    handling: Handling,
    api: Arc<Api>,
    spool: SpoolResult,
    signature: Value,
  ) -> Result<Self> {
    let emitter = Arc::new(EventEmitter::new());
    let (send_tx, send_rx) = mpsc::unbounded_channel::<OutMsg>();
    let last_disconnect_reason = Arc::new(RwLock::new(String::new()));

    let emitter_c = emitter.clone();
    let ldr_c = last_disconnect_reason.clone();
    tokio::spawn(async move {
      if let Err(e) = ribbon_task(
        token,
        handling,
        api,
        spool,
        signature,
        send_rx,
        emitter_c.clone(),
      )
      .await
      {
        *ldr_c.write().await = e.to_string();
        emitter_c.emit("client.fail", json!({ "message": e.to_string() }));
        emitter_c.emit("client.dead", json!(e.to_string()));
      }
    });

    Ok(Self {
      emitter,
      send_tx,
      last_disconnect_reason,
    })
  }

  /// Send a command to the ribbon server.
  pub fn send(&self, command: &str, data: Value) {
    let _ = self.send_tx.send(OutMsg::Send(command.to_string(), data));
  }

  /// Gracefully close the connection.
  pub fn destroy(&self) {
    let _ = self.send_tx.send(OutMsg::Die);
  }

  /// Register a callback for an event. Returns a handle — drop it to cancel.
  pub fn on<F>(&self, command: &str, callback: F) -> tokio::task::JoinHandle<()>
  where
    F: Fn(Value) + Send + 'static,
  {
    self.emitter.on(command, callback)
  }

  /// Wait for the next occurrence of an event.
  pub async fn once(&self, command: &str) -> Option<Value> {
    self.emitter.once(command).await
  }
}

// ── Background task ──────────────────────────────────────────────────────────

async fn ribbon_task(
  token: String,
  handling: Handling,
  api: Arc<Api>,
  spool: SpoolResult,
  signature: Value,
  mut send_rx: mpsc::UnboundedReceiver<OutMsg>,
  emitter: Arc<EventEmitter>,
) -> Result<()> {
  let uri = format!("wss://{}/ribbon/{}", spool.host, spool.endpoint);

  let tls = TlsConnector::builder()
    .request_alpns(&["http/1.1"])
    .build()
    .map_err(|e| TriangleError::Ribbon(format!("TLS build failed: {e}")))?;
  let connector = Connector::NativeTls(tls.into());

  let ws_key = {
    let raw: [u8; 16] = rand::random();
    base64::engine::general_purpose::STANDARD.encode(raw)
  };

  let mut req_builder = Request::builder()
    .uri(&uri)
    .header("Host", spool.host.as_str())
    .header("User-Agent", crate::utils::USER_AGENT)
    .header("Origin", "https://tetr.io")
    .header("Upgrade", "websocket")
    .header("Connection", "Upgrade")
    .header("Sec-WebSocket-Key", &ws_key)
    .header("Sec-WebSocket-Version", "13");

  if !spool.token.is_empty() {
    req_builder = req_builder.header("Sec-WebSocket-Protocol", &spool.token);
  }

  let request = req_builder
    .body(())
    .map_err(|e| TriangleError::Ribbon(format!("failed to build request: {e}")))?;

  let (ws_stream, _) = connect_async_tls_with_config(request, None, false, Some(connector))
    .await
    .map_err(|e| TriangleError::Ribbon(format!("WebSocket connect failed: {e}")))?;

  let (mut ws_sink, mut ws_source) = ws_stream.split();

  let inner = Arc::new(Mutex::new(Inner {
    received_id: 0,
    sent_id: 0,
    sent_queue: VecDeque::new(),
    received_queue: VecDeque::new(),
    session: Session::default(),
  }));

  // ── encode/send helper ───────────────────────────────────────────────

  let encode_msg = |command: &str, data: Option<&Value>| -> String {
    match data {
      Some(d) => serde_json::to_string(&json!({ "command": command, "data": d })),
      None => serde_json::to_string(&json!({ "command": command })),
    }
    .unwrap_or_default()
  };

  // Initiate session (server waits for this before sending `session` message)
  let _ = ws_sink
    .send(Message::Text(encode_msg("new", None).into()))
    .await;

  // main event loop
  loop {
    tokio::select! {
        // incoming WebSocket message
        msg = ws_source.next() => {
            match msg {
                None => {
                    emitter.emit("client.close", json!("connection closed"));
                    break;
                }
                Some(Err(e)) => {
                    emitter.emit("client.fail", json!({ "message": e.to_string() }));
                    return Err(TriangleError::Ribbon(e.to_string()));
                }
                Some(Ok(Message::Text(text))) => {
                    let parsed: Value = match serde_json::from_str(&text) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    let command = parsed["command"].as_str().unwrap_or("").to_string();
                    let data = parsed["data"].clone();
                    let id = parsed["id"].as_u64().and_then(|n| if n > 0 { Some(n as u32) } else { None });

                    // sequence-id accounting
                    if let Some(mid) = id {
                        let mut locked = inner.lock().await;
                        if mid <= locked.received_id {
                            continue;
                        }
                        if mid == locked.received_id + 1 {
                            locked.received_id = mid;
                        } else {
                            // out-of-order — queue for later
                            locked.received_queue.push_back((mid, parsed.clone()));
                            continue;
                        }
                        drop(locked);

                        // drain ordered queue
                        {
                            let mut locked = inner.lock().await;
                            let mut queue: Vec<(u32, Value)> =
                                locked.received_queue.drain(..).collect();
                            queue.sort_by_key(|(id, _)| *id);
                            for (nid, pkt) in queue {
                                if nid != locked.received_id + 1 {
                                    locked.received_queue.push_back((nid, pkt));
                                    continue;
                                }
                                locked.received_id = nid;
                                let cmd = pkt["command"].as_str().unwrap_or("").to_string();
                                let d = pkt["data"].clone();
                                drop(locked);
                                emitter.emit(&cmd, d);
                                locked = inner.lock().await;
                            }
                        }
                    }

                    // handle protocol messages
                    match command.as_str() {
                        "session" => {
                            let ribbon_id = data["ribbonid"].as_str().map(|s| s.to_string());
                            let token_id = data["tokenid"]
                                .as_str()
                                .filter(|s| !s.is_empty())
                                .map(|s| s.to_string());

                            let is_new_session = {
                                let mut locked = inner.lock().await;
                                let was_null = locked.session.token_id.is_none();
                                locked.session.ribbon_id = ribbon_id;
                                locked.session.token_id = token_id.clone();
                                was_null
                            };

                            if is_new_session {
                                // brand-new session — authorize
                                let msg = encode_msg(
                                    "server.authorize",
                                    Some(&json!({
                                        "token": token,
                                        "handling": handling,
                                        "signature": signature,
                                    })),
                                );
                                let _ = ws_sink.send(Message::Text(msg.into())).await;
                            } else {
                                // resuming — resend unacknowledged packets
                                let queue: Vec<String> = {
                                    inner.lock().await.sent_queue
                                        .iter()
                                        .map(|(_, s)| s.clone())
                                        .collect()
                                };
                                let resend = encode_msg(
                                    "packets",
                                    Some(&json!({ "packets": queue })),
                                );
                                let _ = ws_sink.send(Message::Text(resend.into())).await;
                            }
                        }

                        "server.authorize" => {
                            if data["success"].as_bool().unwrap_or(false) {
                                // announce online presence
                                let presence =
                                    encode_msg("social.presence", Some(&json!({ "status": "online", "detail": "menus" })));
                                let _ = ws_sink.send(Message::Text(presence.into())).await;

                                emitter.emit("client.ready", json!({
                                    "social": data["social"],
                                }));
                            } else {
                                emitter.emit(
                                    "client.error",
                                    json!("Failure to authorize ribbon"),
                                );
                            }
                            emitter.emit(&command, data);
                        }

                        "server.migrate" => {
                            // server wants us to switch to a different endpoint -- just close
                            emitter.emit("client.close", json!("server migration"));
                            break;
                        }

                        "kick" | "nope" => {
                            let reason = data["reason"].as_str().unwrap_or("kicked").to_string();
                            emitter.emit("client.dead", json!(reason));
                            let _ = ws_sink.send(Message::Close(None)).await;
                            break;
                        }

                        "packets" => {
                            // resent packets from server-side queue — process session init inline
                            if let Some(packets) = data["packets"].as_array() {
                                for pkt in packets {
                                    let pcmd = pkt["command"].as_str().unwrap_or("").to_string();
                                    let pd = pkt["data"].clone();
                                    match pcmd.as_str() {
                                        "session" => {
                                            let ribbon_id = pd["ribbonid"].as_str().map(|s| s.to_string());
                                            let token_id = pd["tokenid"].as_str().filter(|s| !s.is_empty()).map(|s| s.to_string());
                                            let is_new = {
                                                let mut locked = inner.lock().await;
                                                let was_null = locked.session.token_id.is_none();
                                                locked.session.ribbon_id = ribbon_id;
                                                locked.session.token_id = token_id;
                                                was_null
                                            };
                                            if is_new {
                                                let msg = encode_msg("server.authorize", Some(&json!({
                                                    "token": token,
                                                    "handling": handling,
                                                    "signature": signature,
                                                })));
                                                let _ = ws_sink.send(Message::Text(msg.into())).await;
                                            }
                                        }
                                        _ => {
                                            emitter.emit(&pcmd, pd);
                                        }
                                    }
                                }
                            }
                        }

                        _ => {
                            emitter.emit(&command, data);
                        }
                    }
                }
                Some(Ok(Message::Ping(payload))) => {
                    let _ = ws_sink.send(Message::Pong(payload)).await;
                }
                Some(Ok(Message::Close(_))) => {
                    emitter.emit("client.close", json!("connection closed"));
                    break;
                }
                Some(Ok(_)) => {}
            }
        }

        // outgoing message from caller
        msg = send_rx.recv() => {
            match msg {
                None | Some(OutMsg::Die) => {
                    let _ = ws_sink.send(Message::Close(None)).await;
                    break;
                }
                Some(OutMsg::Send(command, data)) => {
                    let text = encode_msg(&command, Some(&data));
                    let _ = ws_sink.send(Message::Text(text.into())).await;
                }
            }
        }
    }
  }

  Ok(())
}
