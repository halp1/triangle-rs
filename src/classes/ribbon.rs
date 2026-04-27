use std::{
  collections::VecDeque,
  sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
  },
  time::{Duration, Instant},
};

use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use native_tls::TlsConnector;
use serde_json::{Value, json};
use tokio::sync::{RwLock, mpsc};
use tokio_tungstenite::{
  Connector, connect_async_tls_with_config,
  tungstenite::{Message, http::Request},
};

use crate::{
  error::TriangleError,
  types::game::Handling,
  utils::{Api, EventEmitter, api::SpoolResult},
};

// ── Protocol constants ────────────────────────────────────────────────────────

const CACHE_MAXSIZE: usize = 4096;
const PING_INTERVAL_MS: u64 = 2500;
const MAX_RECONNECTS: u32 = 20;

// ── Connection flags (mirrors Ribbon.FLAGS in TypeScript) ─────────────────────

const FLAG_ALIVE: u32 = 1;
const FLAG_SUCCESSFUL: u32 = 2;
const FLAG_CONNECTING: u32 = 4;
const FLAG_FAST_PING: u32 = 8;
const FLAG_TIMING_OUT: u32 = 16;
const FLAG_DEAD: u32 = 32;

// ── Close-code → reason mapping ───────────────────────────────────────────────

fn close_code_reason(code: u16) -> &'static str {
  match code {
    1000 => "ribbon closed normally",
    1001 => "client closed ribbon",
    1002 => "protocol error",
    1003 => "protocol violation",
    1005 => "no error provided",
    1006 => "ribbon lost",
    1007 => "payload data corrupted",
    1008 => "protocol violation",
    1009 => "too much data",
    1010 => "negotiation error",
    1011 => "server error",
    1012 => "server restarting",
    1013 => "temporary error",
    1014 => "bad gateway",
    1015 => "TLS error",
    _ => "unknown",
  }
}

// ── Internal types ────────────────────────────────────────────────────────────

#[derive(Default, Clone)]
struct Session {
  ribbon_id: Option<String>,
  token_id: Option<String>,
}

#[allow(clippy::large_enum_variant)]
enum OutMsg {
  Send(String, Value),
  Die,
  Disconnect,
}

// ── Public handle ─────────────────────────────────────────────────────────────

/// WebSocket connection to the TETR.IO ribbon.
///
/// Uses JSON transport only (binary/amber excluded per project scope).
pub struct Ribbon {
  /// All ribbon events are broadcast here.
  pub emitter: Arc<EventEmitter>,
  send_tx: mpsc::UnboundedSender<OutMsg>,
  /// Reason from the last disconnect / kick.
  pub last_disconnect_reason: Arc<RwLock<String>>,
  /// When `true` the ribbon pings every 2.5 s instead of every 5 s.
  faster_ping: Arc<AtomicBool>,
  /// Last round-trip ping time in milliseconds.
  ping_ms: Arc<AtomicU64>,
}

impl Ribbon {
  /// Connect to TETR.IO and start the ribbon background task.
  ///
  /// Returns immediately after spawning; subscribe to `client.ready` /
  /// `client.fail` on `emitter` to wait for handshake completion.
  pub async fn connect(
    token: String,
    handling: Handling,
    api: Arc<Api>,
    spool: SpoolResult,
    signature: Value,
  ) -> Result<Self, TriangleError> {
    let emitter = Arc::new(EventEmitter::new());
    let (send_tx, send_rx) = mpsc::unbounded_channel::<OutMsg>();
    let last_disconnect_reason = Arc::new(RwLock::new("ribbon lost".to_string()));
    let faster_ping = Arc::new(AtomicBool::new(false));
    let ping_ms = Arc::new(AtomicU64::new(0));

    let emitter_bg = emitter.clone();
    let ldr_bg = last_disconnect_reason.clone();
    let faster_ping_bg = faster_ping.clone();
    let ping_ms_bg = ping_ms.clone();

    tokio::spawn(async move {
      ribbon_manager(
        token,
        handling,
        api,
        spool,
        signature,
        send_rx,
        emitter_bg,
        ldr_bg,
        faster_ping_bg,
        ping_ms_bg,
      )
      .await;
    });

    Ok(Self {
      emitter,
      send_tx,
      last_disconnect_reason,
      faster_ping,
      ping_ms,
    })
  }

  /// Send a command to the ribbon server.
  pub fn send(&self, command: &str, data: Value) {
    let _ = self.send_tx.send(OutMsg::Send(command.to_string(), data));
  }

  /// Return a cloneable send function for passing to Game / Room.
  pub fn make_send_fn(&self) -> Arc<dyn Fn(String, Value) + Send + Sync> {
    let tx = self.send_tx.clone();
    Arc::new(move |cmd: String, data: Value| {
      let _ = tx.send(OutMsg::Send(cmd, data));
    })
  }

  /// Gracefully close the connection (sends `die` to the server).
  pub fn destroy(&self) {
    let _ = self.send_tx.send(OutMsg::Die);
  }

  /// Close the current connection without permanently dying — triggers reconnect.
  pub fn disconnect(&self) {
    let _ = self.send_tx.send(OutMsg::Disconnect);
  }

  /// Register a persistent callback for an event. Returns a handle — abort to cancel.
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

  /// Toggle faster-ping mode (every 2.5 s vs every 5 s).
  /// Called by `Game` to detect disconnects faster.
  pub fn set_faster_ping(&self, value: bool) {
    self.faster_ping.store(value, Ordering::Relaxed);
  }

  /// Last measured round-trip ping in milliseconds.
  pub fn ping(&self) -> u64 {
    self.ping_ms.load(Ordering::Relaxed)
  }
}

// ── JSON encode helper ────────────────────────────────────────────────────────

fn encode_msg(command: &str, data: Option<&Value>) -> String {
  match data {
    Some(d) => serde_json::to_string(&json!({ "command": command, "data": d })),
    None => serde_json::to_string(&json!({ "command": command })),
  }
  .unwrap_or_default()
}

// ── Background manager (reconnect loop) ───────────────────────────────────────

async fn ribbon_manager(
  token: String,
  handling: Handling,
  api: Arc<Api>,
  initial_spool: SpoolResult,
  signature: Value,
  mut send_rx: mpsc::UnboundedReceiver<OutMsg>,
  emitter: Arc<EventEmitter>,
  ldr: Arc<RwLock<String>>,
  faster_ping: Arc<AtomicBool>,
  ping_ms: Arc<AtomicU64>,
) {
  let mut session = Session::default();
  let mut received_id: u32 = 0;
  let mut received_queue: Vec<(u32, Value)> = Vec::new();
  let mut flags: u32 = 0;
  let mut reconnect_count: u32 = 0;
  let mut reconnect_penalty: u64 = 0;
  let mut last_reconnect = Instant::now() - Duration::from_secs(10);
  let mut endpoint_override: Option<String> = None;

  'outer: loop {
    if flags & FLAG_DEAD != 0 {
      break;
    }

    // Re-fetch spool on every connect attempt (mirrors TS #connect behaviour).
    let spool = match api.spool().await {
      Ok(s) => SpoolResult {
        host: s.host,
        endpoint: endpoint_override.take().unwrap_or(s.endpoint),
        token: s.token,
      },
      Err(_) => SpoolResult {
        host: initial_spool.host.clone(),
        endpoint: endpoint_override
          .take()
          .unwrap_or_else(|| initial_spool.endpoint.clone()),
        token: initial_spool.token.clone(),
      },
    };

    let uri = format!("wss://{}/ribbon/{}", spool.host, spool.endpoint);

    flags |= FLAG_CONNECTING;

    // ── Build TLS + WebSocket ────────────────────────────────────────────────
    let connect_result = (|| async {
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

      connect_async_tls_with_config(request, None, false, Some(connector))
        .await
        .map_err(|e| TriangleError::Ribbon(format!("WebSocket connect failed: {e}")))
    })()
    .await;

    let (ws_stream, _) = match connect_result {
      Ok(pair) => pair,
      Err(_) => {
        // Update disconnect reason for failed connect
        let reason = if flags & FLAG_SUCCESSFUL == 0 {
          "failed to connect"
        } else if flags & FLAG_TIMING_OUT != 0 {
          "ping timeout"
        } else {
          "ribbon lost"
        };
        *ldr.write().await = reason.to_string();

        if let Some(wait) = reconnect_backoff(
          &mut flags,
          &mut reconnect_count,
          &mut reconnect_penalty,
          &mut last_reconnect,
        ) {
          tokio::time::sleep(Duration::from_millis(wait)).await;
          continue 'outer;
        } else {
          let r = ldr.read().await.clone();
          emitter.emit("client.dead", json!(r));
          break 'outer;
        }
      }
    };

    // ── Connected ────────────────────────────────────────────────────────────
    flags |= FLAG_ALIVE | FLAG_SUCCESSFUL;
    flags &= !FLAG_TIMING_OUT;

    let (mut ws_sink, mut ws_source) = ws_stream.split();

    // Send session opener: "new" (first time) or "session" (resume)
    let opener = if session.token_id.is_none() {
      encode_msg("new", None)
    } else {
      encode_msg(
        "session",
        Some(&json!({
          "ribbonid": session.ribbon_id,
          "tokenid": session.token_id,
        })),
      )
    };
    let _ = ws_sink.send(Message::Text(opener.into())).await;

    // Ping state
    let mut ping_heartbeat: u32 = 0;
    let mut alive_since_last_ping = true;
    let mut last_ping_sent = Instant::now();
    let mut ping_interval = tokio::time::interval(Duration::from_millis(PING_INTERVAL_MS));
    ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // Per-connection sent queue (only populated if server sends F_ID packets;
    // empty for JSON transport, kept for session-resume `packets` command).
    let mut sent_queue: VecDeque<String> = VecDeque::new();

    let mut endpoint_switch: Option<String> = None;
    let mut close_reason: Option<String> = None;

    // ── Inner event loop ─────────────────────────────────────────────────────
    'inner: loop {
      tokio::select! {
        biased;

        // Outgoing messages from caller
        msg = send_rx.recv() => {
          match msg {
            None | Some(OutMsg::Die) => {
              // Send "die" to server before closing
              let die_msg = encode_msg("die", None);
              let _ = ws_sink.send(Message::Text(die_msg.into())).await;
              let _ = ws_sink.send(Message::Close(None)).await;
              let r = ldr.read().await.clone();
              emitter.emit("client.dead", json!(r));
              break 'outer;
            }
            Some(OutMsg::Disconnect) => {
              let _ = ws_sink.send(Message::Close(None)).await;
              break 'inner;
            }
            Some(OutMsg::Send(command, data)) => {
              // Drop messages while the handshake is in progress (mirrors TS JSON behaviour)
              if flags & FLAG_CONNECTING == 0 && flags & FLAG_DEAD == 0 {
                let text = encode_msg(&command, Some(&data));
                if ws_sink.send(Message::Text(text.into())).await.is_err() {
                  break 'inner;
                }
              }
            }
          }
        }

        // Ping timer
        _ = ping_interval.tick() => {
          ping_heartbeat = ping_heartbeat.wrapping_add(1);
          let fast = faster_ping.load(Ordering::Relaxed);
          let timing_out = flags & FLAG_TIMING_OUT != 0;
          let should_ping = (fast && !timing_out) || (ping_heartbeat % 2 == 0);

          if should_ping {
            if !alive_since_last_ping {
              // No message received since last ping — connection lost
              flags |= FLAG_TIMING_OUT | FLAG_ALIVE | FLAG_CONNECTING;
              close_reason = Some("ping timeout".to_string());
              break 'inner;
            }
            alive_since_last_ping = false;
            let ping_msg = encode_msg("ping", Some(&json!({ "recvid": received_id })));
            last_ping_sent = Instant::now();
            if ws_sink.send(Message::Text(ping_msg.into())).await.is_err() {
              break 'inner;
            }
          }
        }

        // Incoming WebSocket message
        msg = ws_source.next() => {
          match msg {
            None => {
              break 'inner;
            }
            Some(Err(_)) => {
              break 'inner;
            }
            Some(Ok(Message::Text(text))) => {
              alive_since_last_ping = true;
              flags &= !FLAG_TIMING_OUT;

              let parsed: Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(_) => continue 'inner,
              };

              let command = parsed["command"].as_str().unwrap_or("").to_string();
              let data = parsed["data"].clone();
              let id = parsed["id"].as_u64().and_then(|n| {
                if n > 0 { Some(n as u32) } else { None }
              });

              let should_close = run_message(
                &command,
                data,
                id,
                &mut received_id,
                &mut received_queue,
                &mut session,
                &mut flags,
                &mut sent_queue,
                &mut endpoint_switch,
                &mut close_reason,
                &token,
                &handling,
                &signature,
                &emitter,
                &ldr,
                &ping_ms,
                last_ping_sent,
                &uri,
                &mut ws_sink,
              )
              .await;

              // Process the ordered receive queue after every message
              if received_queue.len() > CACHE_MAXSIZE {
                emitter.emit("client.dead", json!("too many lost packets"));
                close_reason = Some("too many lost packets".to_string());
                break 'inner;
              }
              process_queue(
                &mut received_id,
                &mut received_queue,
                &mut session,
                &mut flags,
                &mut sent_queue,
                &mut endpoint_switch,
                &mut close_reason,
                &token,
                &handling,
                &signature,
                &emitter,
                &ldr,
                &ping_ms,
                last_ping_sent,
                &uri,
                &mut ws_sink,
              )
              .await;

              if should_close {
                break 'inner;
              }
              if endpoint_switch.is_some() {
                break 'inner;
              }
            }
            Some(Ok(Message::Ping(payload))) => {
              let _ = ws_sink.send(Message::Pong(payload)).await;
            }
            Some(Ok(Message::Close(frame))) => {
              let code = frame.as_ref().map(|f| u16::from(f.code)).unwrap_or(1006);
              let reason = close_code_reason(code);
              if reason == "ribbon lost" {
                if flags & FLAG_TIMING_OUT != 0 {
                  close_reason = Some("ping timeout".to_string());
                } else if flags & FLAG_SUCCESSFUL == 0 {
                  close_reason = Some("failed to connect".to_string());
                } else {
                  close_reason = Some(reason.to_string());
                }
              } else {
                close_reason = Some(reason.to_string());
              }
              break 'inner;
            }
            Some(Ok(_)) => {}
          }
        }
      }
    }
    // ── End inner loop ───────────────────────────────────────────────────────

    if flags & FLAG_DEAD != 0 {
      break 'outer;
    }

    if let Some(ep) = endpoint_switch.take() {
      // server.migrate: switch to a new endpoint and reconnect immediately
      endpoint_override = Some(ep);
      flags |= FLAG_CONNECTING;
      tokio::time::sleep(Duration::from_millis(5)).await;
      continue 'outer;
    }

    // Update last-disconnect reason
    if let Some(reason) = close_reason.take() {
      *ldr.write().await = reason;
    } else {
      // Derive reason from flags if no explicit reason was set
      let reason = if flags & FLAG_TIMING_OUT != 0 {
        "ping timeout"
      } else if flags & FLAG_SUCCESSFUL == 0 {
        "failed to connect"
      } else {
        "ribbon lost"
      };
      *ldr.write().await = reason.to_string();
    }

    if let Some(wait) = reconnect_backoff(
      &mut flags,
      &mut reconnect_count,
      &mut reconnect_penalty,
      &mut last_reconnect,
    ) {
      tokio::time::sleep(Duration::from_millis(wait)).await;
    } else {
      let r = ldr.read().await.clone();
      emitter.emit("client.dead", json!(r));
      break 'outer;
    }
  }
}

// ── Reconnect backoff ─────────────────────────────────────────────────────────

/// Returns `Some(wait_ms)` if a reconnect should be attempted, `None` if the
/// ribbon should be permanently closed.
fn reconnect_backoff(
  flags: &mut u32,
  count: &mut u32,
  penalty: &mut u64,
  last: &mut Instant,
) -> Option<u64> {
  if last.elapsed() > Duration::from_millis(4000) {
    *count = 0;
  }
  *last = Instant::now();

  if *count >= MAX_RECONNECTS || (*flags & FLAG_DEAD != 0) {
    return None;
  }

  let wait = *penalty + 5 + 100 * (*count as u64);
  *penalty = 0;
  *count += 1;
  *flags |= FLAG_CONNECTING;
  Some(wait)
}

// ── Message processing ────────────────────────────────────────────────────────

/// Drains the ordered receive queue, running all consecutive messages.
/// Returns `true` if the connection should close.
async fn process_queue<S>(
  received_id: &mut u32,
  received_queue: &mut Vec<(u32, Value)>,
  session: &mut Session,
  flags: &mut u32,
  sent_queue: &mut VecDeque<String>,
  endpoint_switch: &mut Option<String>,
  close_reason: &mut Option<String>,
  token: &str,
  handling: &Handling,
  signature: &Value,
  emitter: &Arc<EventEmitter>,
  ldr: &Arc<RwLock<String>>,
  ping_ms: &Arc<AtomicU64>,
  last_ping_sent: Instant,
  uri: &str,
  ws_sink: &mut S,
) where
  S: SinkExt<Message> + Unpin,
{
  if received_queue.is_empty() {
    return;
  }

  received_queue.sort_unstable_by_key(|(id, _)| *id);

  let i = 0;
  while i < received_queue.len() {
    let (id, _) = &received_queue[i];
    let id = *id;

    if id <= *received_id {
      received_queue.remove(i);
      continue;
    }
    if id != *received_id + 1 {
      break;
    }

    let (_, pkt) = received_queue.remove(i);
    let cmd = pkt["command"].as_str().unwrap_or("").to_string();
    let d = pkt["data"].clone();
    let should_close = run_message(
      &cmd,
      d,
      Some(id),
      received_id,
      &mut Vec::new(),
      session,
      flags,
      sent_queue,
      endpoint_switch,
      close_reason,
      token,
      handling,
      signature,
      emitter,
      ldr,
      ping_ms,
      last_ping_sent,
      uri,
      ws_sink,
    )
    .await;

    if should_close || endpoint_switch.is_some() {
      break;
    }
  }
}

/// Process a single incoming message: handle sequence ID accounting, run
/// the command, then emit the raw event.  Returns `true` if the inner loop
/// should break (kick / nope / permanent close).
#[allow(clippy::too_many_arguments)]
async fn run_message<S>(
  command: &str,
  data: Value,
  id: Option<u32>,
  received_id: &mut u32,
  received_queue: &mut Vec<(u32, Value)>,
  session: &mut Session,
  flags: &mut u32,
  sent_queue: &mut VecDeque<String>,
  endpoint_switch: &mut Option<String>,
  close_reason: &mut Option<String>,
  token: &str,
  handling: &Handling,
  signature: &Value,
  emitter: &Arc<EventEmitter>,
  ldr: &Arc<RwLock<String>>,
  ping_ms: &Arc<AtomicU64>,
  last_ping_sent: Instant,
  uri: &str,
  ws_sink: &mut S,
) -> bool
where
  S: SinkExt<Message> + Unpin,
{
  // Sequence ID accounting
  if let Some(mid) = id {
    if mid <= *received_id {
      return false; // already seen
    }
    if mid == *received_id + 1 {
      *received_id = mid;
    } else {
      // Out-of-order: stash for later
      let mut pkt = json!({ "command": command, "data": data });
      pkt["id"] = json!(mid);
      received_queue.push((mid, pkt));
      return false;
    }
  }

  let mut should_close = false;

  match command {
    "session" => {
      let ribbon_id = data["ribbonid"].as_str().map(|s| s.to_string());
      let token_id = data["tokenid"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

      // Clear CONNECTING flag — handshake in progress
      *flags &= !FLAG_CONNECTING;

      session.ribbon_id = ribbon_id;

      if session.token_id.is_some() {
        // Resuming session: resend unacknowledged packets
        let packets_list: Vec<Value> = sent_queue
          .iter()
          .map(|s| Value::String(s.clone()))
          .collect();
        let resend = encode_msg("packets", Some(&json!({ "packets": packets_list })));
        let _ = ws_sink.send(Message::Text(resend.into())).await;
      } else {
        // New session: authorize
        let auth = encode_msg(
          "server.authorize",
          Some(&json!({
            "token": token,
            "handling": handling,
            "signature": signature,
          })),
        );
        let _ = ws_sink.send(Message::Text(auth.into())).await;
      }

      // tokenID is set AFTER the authorize/packets decision (mirrors TS)
      session.token_id = token_id;
    }

    "ping" => {
      // Server echoes our ping: update pinger time.
      // In JSON mode, sent_queue is always empty (no F_ID packets), so the
      // recvid trim is a no-op — but we clear it to maintain correct semantics.
      let _recv_id = data["recvid"].as_u64().unwrap_or(0) as u32;
      sent_queue.clear();
      let elapsed = last_ping_sent.elapsed().as_millis() as u64;
      ping_ms.store(elapsed, Ordering::Relaxed);
    }

    "kick" => {
      let reason = data["reason"].as_str().unwrap_or("kicked").to_string();
      *ldr.write().await = "server closed ribbon".to_string();
      *flags |= FLAG_DEAD;
      emitter.emit("client.dead", json!("server closed ribbon"));
      // Send "die" then close
      let die_msg = encode_msg("die", None);
      let _ = ws_sink.send(Message::Text(die_msg.into())).await;
      let _ = ws_sink.send(Message::Close(None)).await;
      *close_reason = Some(format!("kicked: {reason}"));
      should_close = true;
    }

    "nope" => {
      let reason = data["reason"].as_str().unwrap_or("nope").to_string();
      *ldr.write().await = reason.clone();
      *flags |= FLAG_DEAD;
      emitter.emit("client.dead", json!(reason.clone()));
      let die_msg = encode_msg("die", None);
      let _ = ws_sink.send(Message::Text(die_msg.into())).await;
      let _ = ws_sink.send(Message::Close(None)).await;
      *close_reason = Some(reason);
      should_close = true;
    }

    "packets" => {
      // Server resent our unacknowledged packets — re-process each one
      if let Some(packets) = data["packets"].as_array() {
        for pkt in packets.clone() {
          let pcmd = pkt["command"].as_str().unwrap_or("").to_string();
          let pd = pkt["data"].clone();
          let pid = pkt["id"]
            .as_u64()
            .and_then(|n| if n > 0 { Some(n as u32) } else { None });
          // Use a throwaway sub-queue: packets inside `packets` don't re-enqueue
          let _ = Box::pin(run_message(
            &pcmd,
            pd,
            pid,
            received_id,
            &mut Vec::new(),
            session,
            flags,
            sent_queue,
            endpoint_switch,
            close_reason,
            token,
            handling,
            signature,
            emitter,
            ldr,
            ping_ms,
            last_ping_sent,
            uri,
            ws_sink,
          ))
          .await;
        }
      }
    }

    "server.authorize" => {
      if data["success"].as_bool().unwrap_or(false) {
        // Announce online presence
        let presence = encode_msg(
          "social.presence",
          Some(&json!({ "status": "online", "detail": "menus" })),
        );
        let _ = ws_sink.send(Message::Text(presence.into())).await;

        emitter.emit(
          "client.ready",
          json!({
            "endpoint": uri,
            "social": data["social"],
          }),
        );
      } else {
        emitter.emit("client.error", json!("Failure to authorize ribbon"));
      }
    }

    "server.migrate" => {
      let ep = data["endpoint"]
        .as_str()
        .unwrap_or("")
        .replace("/ribbon/", "");
      *endpoint_switch = Some(ep);
      // Do not set should_close — outer loop handles the switch
    }

    "server.migrated" => {
      // No-op
    }

    _ => {}
  }

  // Always emit the raw event (mirrors TS: always called after the switch block)
  emitter.emit(command, data);

  should_close
}
