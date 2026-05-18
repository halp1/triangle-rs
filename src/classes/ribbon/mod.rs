pub mod bits;
pub mod hook;

pub use hook::Hook;
use serde::{Deserialize, Serialize};

use std::{sync::Arc, time::Duration};

use futures_util::{
  SinkExt, StreamExt,
  stream::{SplitSink, SplitStream},
};
use http::header::{HeaderValue, SEC_WEBSOCKET_PROTOCOL};
use tokio_tungstenite::{
  MaybeTlsStream, WebSocketStream, connect_async,
  tungstenite::{Error, Message, Utf8Bytes, client::IntoClientRequest, http},
};

use crate::{
  classes::ribbon::bits::Bits,
  types::{
    events::{recv, send},
    game::Handling,
    server,
    user::{Me, Role},
  },
  utils::{
    EventEmitter,
    api::{self, Api, core::ApiError},
    events::{AsyncCallback, Event},
  },
};
use bitflags::bitflags;
use parking_lot::Mutex as PMutex;
use tokio::{
  net::TcpStream,
  sync::Mutex,
  time::{Instant, sleep},
};

#[derive(Clone, Debug)]
pub struct Spool {
  pub host: String,
  pub endpoint: String,
  pub token: String,
  pub signature: server::signature::Signature,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoggingLevel {
  All,
  Error,
  None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LogLevel {
  Info,
  Warning,
  Error,
}

pub const CACHE_MAXSIZE: usize = 4096;
pub const BATCH_TIMEOUT: u64 = 25;

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

bitflags! {
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
  pub struct Flags: u32 {
    const ALIVE = 1 << 0;
    const SUCCESSFUL = 1 << 1;
    const CONNECTING = 1 << 2;
    const FAST_PING = 1 << 3;
    const TIMING_OUT = 1 << 4;
    const DEAD = 1 << 5;
    const MIGRATING = 1 << 6;
  }
}

const F_ID_FLAG: u8 = 128;

const SLOW_CODEC_THRESHOLD: Duration = Duration::from_micros(16_670);

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum OutMsg {
  Send(String, serde_json::Value),
  Die,
  Disconnect,
}

#[derive(Debug, Clone)]
pub struct Pinger {
  pub heartbeat: u64,
  pub last: Instant,
  pub time: Duration,
}

#[derive(Debug, Clone)]
pub struct Session {
  pub token_id: String,
  pub ribbon_id: String,
}

#[derive(Debug, Clone)]
pub struct OutPacket {
  pub id: u32,
  pub packet: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct InPacket {
  pub id: Option<u32>,
  pub command: String,
  pub data: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct Options {
  pub logging: LoggingLevel,
  pub spooling: bool,
  pub debug: bool,
}

impl Default for Options {
  fn default() -> Self {
    Self {
      logging: LoggingLevel::Error,
      spooling: true,
      debug: false,
    }
  }
}

#[derive(Serialize, Deserialize)]
pub struct PacketWithoutId {
  pub command: String,
  pub data: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Transport {
  #[default]
  JSON,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportData {
  UTF8(Vec<u8>),
  Binary(Vec<u8>),
}

impl Transport {
  pub fn encode(&self, command: &str, data: serde_json::Value) -> TransportData {
    match self {
      Transport::JSON => TransportData::UTF8(
        serde_json::to_vec(&PacketWithoutId {
          command: command.to_string(),
          data,
        })
        .unwrap_or_else(|_| b"{}".to_vec()),
      ),
    }
  }

  pub fn decode(&self, data: &[u8]) -> serde_json::Value {
    match self {
      Transport::JSON => serde_json::from_slice(data).unwrap_or(serde_json::json!({})),
    }
  }
}

#[derive(Debug, Clone)]
struct RibbonConfig {
  token: String,
  handling: Handling,
  transport: Transport,
  options: Options,
}

#[derive(Debug)]
struct RibbonState {
  spool: Spool,
  me: Me,
  pinger: Pinger,
  session: Session,
  sent_id: u32,
  received_id: u32,
  flags: Flags,
  last_disconnect_reason: String,
  sent_queue: Vec<OutPacket>,
  recv_queue: Vec<InPacket>,
}

#[derive(Debug)]
struct RibbonReconnectState {
  reconnect_handle: Option<tokio::task::JoinHandle<()>>,
  last_reconnect: Instant,
  reconnect_count: u32,
  reconnect_penalty: u32,
}

#[derive(Debug, Clone)]
pub struct Ribbon {
  write: Arc<Mutex<Option<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>>>,
  config: Arc<PMutex<RibbonConfig>>,
  state: Arc<PMutex<RibbonState>>,
  reconnect_state: Arc<Mutex<RibbonReconnectState>>,
  pub api: Arc<Api>,
  pub emitter: EventEmitter,
}

#[derive(Debug, Clone, Default)]
pub struct Params {
  pub options: Options,
  pub token: String,
  pub handling: Handling,
  pub user_agent: String,
  pub transport: Transport,
}

#[derive(Debug, Clone, Default)]
pub struct OptionalParams {
  pub options: Option<Options>,
  pub handling: Option<Handling>,
  pub user_agent: Option<String>,
  pub transport: Option<Transport>,
}

pub use crate::utils::events::WrapError;

impl Ribbon {
  pub async fn new(params: Params) -> Result<Self, ApiError> {
    let api = Api::new(api::Config {
      token: params.token.clone(),
      user_agent: params.user_agent.clone(),
      transport: match params.transport {
        Transport::JSON => api::Transport::Binary,
      },
    });

    let env = api.server.environment().await?;

    let me = api.users.me().await?;

    let ribbon = Self {
      write: Arc::new(Mutex::new(None)),
      config: Arc::new(PMutex::new(RibbonConfig {
        token: params.token,
        handling: params.handling,
        transport: params.transport,
        options: params.options,
      })),
      state: Arc::new(PMutex::new(RibbonState {
        spool: Spool {
          host: "".to_string(),
          endpoint: "".to_string(),
          token: "".to_string(),
          signature: env.signature,
        },
        me,
        pinger: Pinger {
          heartbeat: 0,
          last: Instant::now(),
          time: Duration::from_secs(0),
        },
        session: Session {
          token_id: String::new(),
          ribbon_id: String::new(),
        },
        sent_id: 0,
        received_id: 0,
        flags: Flags::empty(),
        last_disconnect_reason: String::new(),
        sent_queue: Vec::new(),
        recv_queue: Vec::new(),
      })),
      reconnect_state: Arc::new(Mutex::new(RibbonReconnectState {
        reconnect_handle: None,
        last_reconnect: Instant::now(),
        reconnect_count: 0,
        reconnect_penalty: 0,
      })),
      api: Arc::new(api),
      emitter: EventEmitter::new(),
    };

    let ribbon_clone = ribbon.clone();
    tokio::spawn(Box::pin(async move {
      Ribbon::pinger(ribbon_clone).await;
    }));

    Ok(ribbon)
  }

  async fn log(&self, msg: &str, level: LogLevel, force: bool) {
    if level == LogLevel::Error {
      self.emitter.emit_raw(
        "client.ribbon.error",
        serde_json::from_str(msg).unwrap_or(serde_json::json!({"error": msg})),
      );
    } else if level == LogLevel::Warning {
      self.emitter.emit_raw(
        "client.ribbon.warn",
        serde_json::from_str(msg).unwrap_or(serde_json::json!({"warn": msg})),
      );
    } else {
      self.emitter.emit_raw(
        "client.ribbon.log",
        serde_json::from_str(msg).unwrap_or(serde_json::json!({"log": msg})),
      );
    }

    let logging = self.config.lock().options.logging;

    if logging == LoggingLevel::None || (logging == LoggingLevel::Error && !force) {
      return;
    }

    // let prefix = match level {
    //   LogLevel::Info => "[triangle-rs]".blue().to_string(),
    //   LogLevel::Warning => "[triangle-rs]".yellow().to_string(),
    //   LogLevel::Error => "[triangle-rs]".red().to_string(),
    // };
    // match level {
    //   LogLevel::Info => println!("{} {}", prefix, msg),
    //   LogLevel::Warning | LogLevel::Error => eprintln!("{} {}", prefix, msg),
    // }

    match level {
      LogLevel::Info => tracing::info!("{}", msg),
      LogLevel::Warning => tracing::warn!("{}", msg),
      LogLevel::Error => tracing::error!("{}", msg),
    }
  }

  async fn encode(&self, msg: &str, data: serde_json::Value) -> TransportData {
    let transport = self.config.lock().transport.clone();

    let start = Instant::now();

    let res = transport.encode(msg, data);

    let end = Instant::now();
    if end.duration_since(start) > SLOW_CODEC_THRESHOLD {
      self
        .log(
          &format!(
            "Slow encode: {} ({}ms)",
            msg,
            end.duration_since(start).as_millis()
          ),
          LogLevel::Warning,
          true,
        )
        .await;
    }
    res
  }

  async fn decode(&self, data: &[u8]) -> serde_json::Value {
    let start = Instant::now();

    let transport = self.config.lock().transport.clone();

    let res = transport.decode(data);

    let end = Instant::now();
    if end.duration_since(start) > SLOW_CODEC_THRESHOLD {
      self
        .log(
          &format!(
            "Slow decode: {} ({}ms)",
            res["command"].as_str().unwrap_or("unknown"),
            end.duration_since(start).as_millis()
          ),
          LogLevel::Warning,
          true,
        )
        .await;
    }
    res
  }

  pub fn uri(&self, spool: Spool) -> String {
    format!("wss://{}/ribbon/{}", spool.host, spool.endpoint)
  }

  pub fn open(&self) {
    let ribbon = self.clone();
    tokio::spawn(Box::pin(async move {
      ribbon.connect().await.ok();
    }));
  }

  async fn connect(&self) -> Result<(), ApiError> {
    let options = self.config.lock().options.clone();

    let spool = self.api.server.spool(options.spooling).await?;

    let (uri, token, had_successful, host, endpoint) = {
      let mut state = self.state.lock();
      state.spool = Spool {
        host: spool.host.clone(),
        endpoint: if state.spool.endpoint.is_empty() {
          spool.endpoint.clone()
        } else {
          state.spool.endpoint.clone()
        },
        token: spool.token.clone(),
        signature: state.spool.signature.clone(),
      };
      let had_successful = state.flags.contains(Flags::SUCCESSFUL);
      state.flags |= Flags::CONNECTING;
      (
        self.uri(state.spool.clone()),
        state.spool.token.clone(),
        had_successful,
        spool.host.clone(),
        spool.endpoint.clone(),
      )
    };

    self
      .log(
        &format!(
          "Connecting to <{}/{}>",
          host.split(".").into_iter().next().unwrap_or(&host),
          endpoint
        ),
        LogLevel::Info,
        false,
      )
      .await;

    if let Some(mut write) = self.write.lock().await.take() {
      write.close().await.ok();
    }

    let mut request = uri.into_client_request().expect("Invalid WebSocket URL");
    let protocol_header =
      HeaderValue::from_str(token.as_str()).expect("Invalid characters in token/protocol");

    request
      .headers_mut()
      .insert(SEC_WEBSOCKET_PROTOCOL, protocol_header);

    let (stream, _) = match connect_async(request).await {
      Ok(stream) => stream,
      Err(error) => {
        if !had_successful {
          self
            .log(
              &format!("Connection error: {}", error),
              LogLevel::Error,
              false,
            )
            .await;

          return Err(ApiError::Server(format!("Failed to connect: {}", error)));
        } else {
          self
            .log(
              &format!("Connection error (will attempt to reconnect): {}", error),
              LogLevel::Warning,
              false,
            )
            .await;
        }

        let ribbon = self.clone();

        self
          .reconnect_state
          .lock()
          .await
          .reconnect_handle
          .replace(tokio::spawn(Box::pin(async move {
            ribbon.reconnect().await;
          })));

        return Ok(());
      }
    };

    let (write, read) = stream.split();

    {
      let mut state = self.state.lock();
      state.flags |= Flags::ALIVE | Flags::SUCCESSFUL;
      state.flags &= !Flags::TIMING_OUT;
    }

    self.write.lock().await.replace(write);

    let session = self.state.lock().session.clone();

    if session.token_id.is_empty() {
      self.pipe("new", serde_json::json!(null)).await.ok();
    } else {
      self
        .pipe(
          "session",
          serde_json::json!({
            "ribbonid": session.ribbon_id,
            "tokenid": session.token_id,
          }),
        )
        .await
        .ok();
    }

    let ribbon = self.clone();
    tokio::spawn(Box::pin(async move {
      Self::listen(read, ribbon).await;
    }));

    Ok(())
  }

  async fn pipe(&self, command: &str, data: serde_json::Value) -> Result<(), Error> {
    let packet = self.encode(command, data.clone()).await;

    match packet {
      TransportData::UTF8(s) => {
        if let Some(write) = &mut *self.write.lock().await {
          let msg = Message::Text(Utf8Bytes::try_from(s).expect("Failed to convert to UTF-8"));
          write.send(msg).await?;
        }
      }

      TransportData::Binary(b) => {
        if (b[0] & F_ID_FLAG) == 0 {
          if let Some(write) = &mut *self.write.lock().await {
            let msg = Message::Binary(b.into());
            write.send(msg).await?;
          }
        } else {
          let id = {
            let mut state = self.state.lock();
            let id = state.sent_id;
            state.sent_id += 1;
            id
          };

          let mut bits = Bits::from_bytes(b);
          bits
            .seek(8, 1)
            .expect("Fatal error: Failed to write packet ID");
          bits
            .write(id.into(), 24)
            .expect("Fatal error: Failed to write packet ID");
          let packet = bits.into_bytes();

          let should_send = {
            let mut state = self.state.lock();
            state.sent_queue.push(OutPacket {
              id,
              packet: packet.clone(),
            });

            if !state.flags.contains(Flags::CONNECTING) {
              let current_queue_size = state.sent_queue.len();
              state
                .sent_queue
                .drain(..current_queue_size.saturating_sub(CACHE_MAXSIZE));
              true
            } else {
              false
            }
          };

          if should_send && let Some(write) = &mut *self.write.lock().await {
            let msg = Message::Binary(packet.into());
            write.send(msg).await?;
          }
        }
      }
    }

    if command != "ping" && self.config.lock().options.logging == LoggingLevel::All {
      self
        .log(
          &format!(
            "SEND {} {}",
            command,
            serde_json::to_string_pretty(&data).unwrap_or_else(|_| data.to_string())
          ),
          LogLevel::Info,
          false,
        )
        .await;
    }

    self.emitter.emit(send::client::ribbon::Send {
      command: command.into(),
      data: data,
    });

    Ok(())
  }

  pub async fn emit<T: Event>(&self, event: T) -> Result<(), Error> {
    if T::NAME.starts_with("client.") {
      self.emitter.emit(event);
    } else {
      self
        .pipe(
          T::NAME,
          serde_json::to_value(&event).unwrap_or(serde_json::json!({})),
        )
        .await?;
    }
    Ok(())
  }

  pub async fn emit_raw(&self, command: &str, data: serde_json::Value) {
    if command.starts_with("client.") {
      self.emitter.emit_raw(command, data);
    } else {
      self.pipe(command, data).await.ok();
    }
  }

  fn process_message(
    &self,
    msg: serde_json::Value,
  ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
    Box::pin(async move {
      let command = msg["command"].as_str().unwrap_or("");
      let data = &msg["data"];
      let id = msg["id"].as_u64().map(|v| v as u32);

      if let Some(id) = id {
        let received_id = self.state.lock().received_id;
        if id > received_id {
          let packet = InPacket {
            id: Some(id),
            command: command.to_string(),
            data: data.clone(),
          };
          if id == received_id + 1 {
            self.run_message(packet).await;
          } else {
            self.state.lock().recv_queue.push(packet);
          }
        }
      } else {
        self
          .run_message(InPacket {
            id: None,
            command: command.to_string(),
            data: data.clone(),
          })
          .await;
      }
    })
  }

  async fn process_queue(&self) {
    let queue_len = {
      let state = self.state.lock();
      if state.recv_queue.is_empty() {
        return;
      }
      state.recv_queue.len()
    };

    if queue_len > CACHE_MAXSIZE {
      self.close("too many lost packets").await;
      return;
    }

    let packets = {
      let mut state = self.state.lock();

      state.recv_queue.sort_by_key(|p| p.id.unwrap_or(0));

      let mut packets = Vec::new();

      while let Some(packet) = state.recv_queue.first() {
        if let Some(id) = packet.id {
          if id <= state.received_id {
            state.recv_queue.remove(0);
            continue;
          } else if id != state.received_id + 1 {
            break;
          } else {
            state.received_id = id;
            packets.push(state.recv_queue.remove(0));
          }
        } else {
          state.recv_queue.remove(0);
        }
      }

      packets
    };

    for packet in packets {
      self.run_message(packet).await;
    }
  }

  async fn run_message(&self, packet: InPacket) {
    if let Some(id) = packet.id {
      self.state.lock().received_id = id;
    }

    if packet.command != "ping" && packet.command != "packets" {
      self.emitter.emit_raw(
        "client.ribbon.receive",
        serde_json::json!({
          "command": packet.command,
          "data": packet.data,
        }),
      );

      self
        .log(
          &format!(
            "RECEIVE {} {}",
            packet.command,
            serde_json::to_string_pretty(&packet.data).unwrap_or_else(|_| packet.data.to_string())
          ),
          LogLevel::Info,
          false,
        )
        .await;
    }

    // debug validation? idk if its possible/easy

    match packet.command.as_str() {
      "session" => {
        let ribbonid = packet.data["ribbonid"].as_str().unwrap_or("").to_string();
        let tokenid = packet.data["tokenid"].as_str().unwrap_or("").to_string();

        let (session, spool, sent_queue, config) = {
          let mut state = self.state.lock();
          let config = self.config.lock().clone();

          state.flags &= !(Flags::CONNECTING | Flags::MIGRATING);
          state.session.ribbon_id = ribbonid;

          let session = state.session.clone();
          let spool = state.spool.clone();
          let sent_queue = state.sent_queue.clone();
          (session, spool, sent_queue, config)
        };

        if !session.token_id.is_empty() {
          self
            .pipe(
              "packets",
              serde_json::json!({
                "packets": sent_queue.iter().map(|p| match config.transport {
                  Transport::JSON => String::from_utf8(p.packet.clone()).unwrap_or_default(),
                }).collect::<Vec<_>>(),
              }),
            )
            .await
            .ok();
        } else {
          self
            .pipe(
              "server.authorize",
              serde_json::json!({
                "token": config.token,
                "handling": config.handling,
                "signature": spool.signature
              }),
            )
            .await
            .ok();
        }

        self.state.lock().session.token_id = tokenid;
      }

      "packets" => {
        for packet in packet.data["packets"].as_array().unwrap_or(&vec![]) {
          let transport = self.config.lock().transport.clone();
          match transport {
            Transport::JSON => {
              self.clone().process_message(packet.clone()).await;
            }
          }
        }
      }

      "ping" => {
        let id = packet.data["recvid"].as_u64().map(|v| v as u32);
        let mut state = self.state.lock();

        state.pinger.time = Instant::now() - state.pinger.last;

        if let Some(id) = id {
          while state.sent_queue.len() > 0 && state.sent_queue[0].id <= id {
            state.sent_queue.remove(0);
          }
        }
      }

      "kick" => {
        let reason = packet.data["reason"]
          .as_str()
          .unwrap_or("unknown")
          .to_string();

        self.state.lock().last_disconnect_reason = "server closed ribbon".into();

        self
          .log(&format!("kicked: {}", reason), LogLevel::Error, true)
          .await;

        self.close("").await;
      }

      "nope" => {
        let reason = packet.data["reason"]
          .as_str()
          .unwrap_or("unknown")
          .to_string();

        self.state.lock().last_disconnect_reason = reason.clone();

        self
          .log(
            &format!("packet rejected: {}", reason),
            LogLevel::Warning,
            true,
          )
          .await;

        self.close("").await;
      }

      "server.authorize" => {
        let data = serde_json::from_value::<recv::server::Authorize>(packet.data.clone());

        match data {
          Ok(data) => {
            let spool = self.state.lock().spool.clone();
            if data.success {
              self.log("Authorized", LogLevel::Info, false).await;

              self
                .emit(send::social::Presence {
                  status: crate::types::social::Status::Online,
                  detail: crate::types::social::Detail::Menus,
                })
                .await
                .ok();

              self
                .emit(send::client::Ready {
                  endpoint: self.uri(spool),
                  social: data.social,
                })
                .await
                .ok();

              let role = self.state.lock().me.role.clone();
              match role {
                Role::Bot | Role::Banned => {}
                _ => {
                  self
                    .api
                    .post::<serde_json::Value>(
                      "reports/submit",
                      serde_json::json!({
                        "target": self.state.lock().me.username.clone(),
                        "type": "cheating",
                        "reason": "non-bot account used with triangle-rs, auto report"
                      }),
                    )
                    .await
                    .ok();
                }
              };
            } else {
              // TODO: close
              // this.emitter.emit("client.error", "Failure to authorize ribbon");
            }
          }
          Err(e) => {
            self
              .log(
                &format!("Failed to parse server.authorize event: {}", e),
                LogLevel::Error,
                true,
              )
              .await;
            // TODO: close
          }
        }
      }

      "server.migrate" => {
        let endpoint = packet.data["endpoint"].as_str().unwrap_or("");

        self
          .log(
            &format!("Migrating to worker {}", endpoint),
            LogLevel::Info,
            false,
          )
          .await;

        self.switch(endpoint.replace("/ribbon/", "").as_str()).await;
      }

      "server.migrated" => {
        self.log("Migration complete", LogLevel::Info, false).await;
      }

      _ => {}
    }

    self
      .emitter
      .emit_raw(packet.command.as_str(), packet.data.clone());
  }

  async fn switch(&self, target: &str) {
    {
      let mut state = self.state.lock();

      state.spool.endpoint = target.to_string();
      state.flags |= Flags::CONNECTING | Flags::MIGRATING;
    }

    sleep(Duration::from_millis(5)).await;

    self.__internal_reconnect().await;
  }

  fn __internal_reconnect(
    &self,
  ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
    Box::pin(async move {
      self
        .reconnect_state
        .lock()
        .await
        .reconnect_handle
        .take()
        .map(|h| h.abort());

      if let Some(mut write) = self.write.lock().await.take() {
        write.close().await.ok();
      }

      let flags = self.state.lock().flags;

      if !flags.contains(Flags::DEAD) {
        let ribbon = self.clone();
        let handle = tokio::spawn(Box::pin(async move {
          ribbon.connect().await.ok();
        }));
        self
          .reconnect_state
          .lock()
          .await
          .reconnect_handle
          .replace(handle);
      }
    })
  }

  async fn reconnect(&self) {
    if self.reconnect_state.lock().await.reconnect_handle.is_some() {
      return;
    }

    if let Some(mut write) = self.write.lock().await.take() {
      write.close().await.ok();
    }

    let mut reconnect_state = self.reconnect_state.lock().await;

    if reconnect_state.last_reconnect.elapsed() > Duration::from_secs(4) {
      reconnect_state.reconnect_count = 0;
    }

    reconnect_state.last_reconnect = Instant::now();

    let flags = self.state.lock().flags;

    if reconnect_state.reconnect_count >= 20 || flags.contains(Flags::DEAD) {
      let reason = if flags.contains(Flags::DEAD) {
        "may not reconnect"
      } else {
        "too many reconnects"
      };

      drop(reconnect_state);

      self.close(reason).await;

      return;
    }

    let wait_time = Duration::from_millis(
      reconnect_state.reconnect_penalty as u64 + 5 + 100 * reconnect_state.reconnect_count as u64,
    );

    let ribbon = self.clone();

    reconnect_state
      .reconnect_handle
      .replace(tokio::spawn(Box::pin(async move {
        sleep(wait_time).await;
        ribbon.__internal_reconnect().await;
      })));

    reconnect_state.reconnect_penalty = 0;
    reconnect_state.reconnect_count += 1;
  }

  pub async fn __internal_close(&self, reason: &str) {
    {
      let mut state = self.state.lock();
      if !reason.is_empty() {
        state.last_disconnect_reason = reason.to_string();
      }

      self
        .emitter
        .emit(send::client::Dead(state.last_disconnect_reason.clone()));
    }

    let write_exists = self.write.lock().await.is_some();

    if write_exists {
      self.emit(send::Die {}).await.ok();
    }

    if let Some(mut write) = self.write.lock().await.take() {
      write.close().await.ok();
    }

    {
      let mut state = self.state.lock();
      state.flags |= Flags::DEAD;
    }

    self
      .reconnect_state
      .lock()
      .await
      .reconnect_handle
      .take()
      .map(|h| h.abort());
  }

  pub async fn close(&self, reason: &str) {
    self.__internal_close(reason).await;
  }

  async fn listen(
    mut stream: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    ribbon: Ribbon,
  ) {
    ribbon
      .log("Listening for messages...", LogLevel::Info, false)
      .await;
    while let Some(message) = stream.next().await {
      match message {
        Ok(msg) => {
          let decoded = ribbon
            .decode(match msg {
              Message::Text(ref s) => s.as_bytes(),
              Message::Binary(ref b) => b,
              Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => continue,
              Message::Close(frame) => {
                if ribbon
                  .state
                  .lock()
                  .flags
                  .intersects(Flags::DEAD | Flags::CONNECTING | Flags::MIGRATING)
                {
                  return;
                }

                println!("CLOSE FRAME RECEIVED: {:?}", frame);

                ribbon
                  .log(
                    "Close frame received, closing connection",
                    LogLevel::Warning,
                    false,
                  )
                  .await;

                let code = frame.as_ref().map(|f| f.code.into()).unwrap_or(1000);
                let reason = close_code_reason(code);
                ribbon.state.lock().last_disconnect_reason = reason.into();
                ribbon.state.lock().flags |= Flags::CONNECTING;
                ribbon.reconnect().await;
                return;
              }
            })
            .await;

          {
            let mut state = ribbon.state.lock();
            state.flags |= Flags::ALIVE;
            state.flags.remove(Flags::TIMING_OUT);
          }

          ribbon.process_message(decoded).await;
          ribbon.process_queue().await;
        }
        Err(e) => {
          println!("Error receiving message: {}", e);
          match e {
            Error::ConnectionClosed => {
              ribbon
                .log("Connection closed by server", LogLevel::Warning, true)
                .await;
              // ribbon.state.lock().flags |= Flags::CONNECTING;
              // ribbon.reconnect().await;

              return;
            }
            _ => {
              ribbon.log(&format!("{}", e), LogLevel::Error, true).await;
            }
          }
          // handle error
        }
      }
    }
		println!("connection closed");
  }

  async fn pinger(ribbon: Ribbon) {
    loop {
      tokio::time::sleep(Duration::from_millis(2500)).await;

      if ribbon.state.lock().flags.contains(Flags::DEAD) {
        return;
      }

      let (should_ping, is_alive) = {
        let mut state = ribbon.state.lock();
        state.pinger.heartbeat += 1;
        let should_ping =
          if state.flags.contains(Flags::FAST_PING) && !state.flags.contains(Flags::TIMING_OUT) {
            true
          } else {
            state.pinger.heartbeat % 2 == 0
          };
        let is_alive = state.flags.contains(Flags::ALIVE);
        if should_ping {
          if !is_alive {
            state.flags |= Flags::TIMING_OUT | Flags::ALIVE | Flags::CONNECTING;
          } else {
            state.flags.remove(Flags::ALIVE);
          }
        }
        (should_ping, is_alive)
      };

      if should_ping {
        if !is_alive {
          ribbon
            .log(
              "Connection timed out, reconnecting...",
              LogLevel::Warning,
              false,
            )
            .await;
          ribbon.reconnect().await;
        } else {
          let write_open = ribbon.write.lock().await.is_some();
          if write_open {
            ribbon.state.lock().pinger.last = Instant::now();
            ribbon
              .pipe(
                "ping",
                serde_json::json!({
                  "recvid": ribbon.state.lock().received_id,
                }),
              )
              .await
              .ok();
          }
        }
      }
    }
  }

  pub async fn set_faster_ping(&self, value: bool) {
    let mut state = self.state.lock();
    if value {
      state.flags |= Flags::FAST_PING;
    } else {
      state.flags &= !Flags::FAST_PING;
    }
  }

  pub async fn wait<T: Event>(&self) -> Option<T> {
    self.emitter.wait::<T>().await
  }

  pub async fn on<T: Event>(
    &self,
    callback: impl AsyncFnOnce(T) -> () + AsyncCallback<T>,
  ) -> tokio::task::JoinHandle<()> {
    self.emitter.on(callback)
  }

  pub async fn once<T: Event>(
    &self,
    callback: impl Fn(T) + Send + Sync + 'static,
  ) -> tokio::task::JoinHandle<()> {
    self.emitter.once(callback)
  }

  pub fn hook(&self) -> Hook {
    Hook::new(self.emitter.clone())
  }

  pub async fn wrap<T: Event>(&self, event: impl Event) -> std::result::Result<T, WrapError> {
    self.wrap_with_error::<T>(event, &["client.error"]).await
  }

  pub async fn wrap_with_error<T: Event>(
    &self,
    event: impl Event,
    error_events: &[&str],
  ) -> std::result::Result<T, WrapError> {
    let cmd = event.name().to_string();
    let data = serde_json::to_value(&event).unwrap_or(serde_json::json!({}));
    let ribbon = self.clone();
    self
      .emitter
      .wrap_with_error::<T>(
        async move { ribbon.emit_raw(&cmd, data).await },
        error_events,
      )
      .await
  }

  pub async fn destroy(&self) {
    self.emitter.destroy();
    self.close("").await;
  }
}
