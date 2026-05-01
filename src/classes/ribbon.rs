use std::{sync::Arc, time::Duration};

use futures_util::{
  SinkExt, StreamExt,
  stream::{SplitSink, SplitStream},
};
use http::header::{HeaderValue, SEC_WEBSOCKET_PROTOCOL};
use tokio_tungstenite::{
  MaybeTlsStream, WebSocketStream, connect_async,
  tungstenite::{Error, Message, client::IntoClientRequest, http},
};

use crate::{
  types::{events, game::Handling, server, user::Me},
  utils::{
    EventEmitter,
    api::{self, Api},
    events::Event,
  },
};
use bitflags::bitflags;
use tokio::{
  net::TcpStream,
  sync::Mutex,
  time::{Instant, sleep},
};

use crate::Result;

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
  }
}

const F_ID_FLAG: u32 = 128;

const SLOW_CODEC_THRESHOLD: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum OutMsg {
  Send(String, serde_json::Value),
  Die,
  Disconnect,
}

#[derive(Debug)]
pub struct Pinger {
  pub heartbeat: u64,
  pub handle: Option<tokio::task::JoinHandle<()>>,
  // something to stop it
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
  pub id: Option<u32>,
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Transport {
  #[default]
  JSON,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportData {
  UTF8(String),
  Binary(Vec<u8>),
}

impl Transport {
  pub fn encode(&self, command: &str, data: serde_json::Value) -> TransportData {
    match self {
      Transport::JSON => TransportData::UTF8(
        serde_json::json!({
          "command": command,
          "data": data,
        })
        .to_string(),
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
  user_agent: String,
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
  config: Arc<Mutex<RibbonConfig>>,
  state: Arc<Mutex<RibbonState>>,
  reconnect_state: Arc<Mutex<RibbonReconnectState>>,
  api: Arc<Api>,
  pub emitter: Arc<Mutex<EventEmitter>>,
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
  pub token: Option<String>,
  pub handling: Option<Handling>,
  pub user_agent: Option<String>,
  pub transport: Option<Transport>,
}

impl From<OptionalParams> for Params {
  fn from(opt: OptionalParams) -> Self {
    Self {
      options: opt.options.unwrap_or_default(),
      token: opt.token.unwrap_or_default(),
      handling: opt.handling.unwrap_or_default(),
      user_agent: opt.user_agent.unwrap_or_default(),
      transport: opt.transport.unwrap_or_default(),
    }
  }
}

impl Ribbon {
  pub async fn new(params: Params) -> Result<Self> {
    let api = Api::new(api::Config {
      token: params.token.clone(),
      user_agent: params.user_agent.clone(),
      transport: match params.transport {
        Transport::JSON => api::Transport::JSON,
      },
    });

    let env = api.server.environment().await?;

    let me = api.users.me().await?;

    Ok(Self {
      write: Arc::new(Mutex::new(None)),
      config: Arc::new(Mutex::new(RibbonConfig {
        token: params.token,
        handling: params.handling,
        user_agent: params.user_agent,
        transport: params.transport,
        options: params.options,
      })),
      state: Arc::new(Mutex::new(RibbonState {
        spool: Spool {
          host: "".to_string(),
          endpoint: "".to_string(),
          token: "".to_string(),
          signature: env.signature,
        },
        me,
        pinger: Pinger {
          heartbeat: 0,
          handle: None,
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
      emitter: Arc::new(Mutex::new(EventEmitter::new())),
    })
  }

  pub async fn log(&self, msg: &str, level: LogLevel, force: bool) {
    if level == LogLevel::Error {
      self.emitter.lock().await.emit_raw(
        "client.ribbon.error",
        serde_json::from_str(msg).unwrap_or(serde_json::json!({"error": msg})),
      );
    } else if level == LogLevel::Warning {
      self.emitter.lock().await.emit_raw(
        "client.ribbon.warn",
        serde_json::from_str(msg).unwrap_or(serde_json::json!({"warn": msg})),
      );
    } else {
      self.emitter.lock().await.emit_raw(
        "client.ribbon.log",
        serde_json::from_str(msg).unwrap_or(serde_json::json!({"log": msg})),
      );
    }

    let logging = self.config.lock().await.options.logging;

    if logging == LoggingLevel::None || (logging == LoggingLevel::Error && !force) {
      return;
    }

    match level {
      LogLevel::Info => println!("[Triangle.rs] {}", msg),
      LogLevel::Warning => eprintln!("[Triangle.rs] {}", msg),
      LogLevel::Error => eprintln!("[Triangle.rs] {}", msg),
    }
  }

  async fn encode(&self, msg: &str, data: serde_json::Value) -> TransportData {
    let start = Instant::now();

    let transport = self.config.lock().await.transport.clone();

    let res = transport.encode(msg, data);

    let end = Instant::now();
    if end.duration_since(start) > SLOW_CODEC_THRESHOLD {
      self.log(
        &format!(
          "Slow encode: {} ({}ms)",
          msg,
          end.duration_since(start).as_millis()
        ),
        LogLevel::Warning,
        true,
      );
    }
    res
  }

  async fn decode(&self, data: &[u8]) -> serde_json::Value {
    let start = Instant::now();

    let transport = self.config.lock().await.transport.clone();

    let res = transport.decode(data);

    let end = Instant::now();
    if end.duration_since(start) > SLOW_CODEC_THRESHOLD {
      self.log(
        &format!(
          "Slow decode: {} ({}ms)",
          res["command"].as_str().unwrap_or("unknown"),
          end.duration_since(start).as_millis()
        ),
        LogLevel::Warning,
        true,
      );
    }
    res
  }

  pub fn uri(&self, spool: Spool) -> String {
    format!("wss://{}/ribbon/{}", spool.host, spool.endpoint)
  }

  pub fn open(&mut self) {
    let mut ribbon = self.clone();
    tokio::spawn(Box::pin(async move {
      ribbon.connect().await.ok();
    }));
  }

  async fn connect(&mut self) -> Result<()> {
    let options = self.config.lock().await.options.clone();

    let mut state = self.state.lock().await;

    let spool = self.api.server.spool(options.spooling).await?;

    state.spool = Spool {
      host: spool.host,
      endpoint: spool.endpoint,
      token: spool.token,
      signature: state.spool.signature.clone(),
    };

    self
      .log(
        format!(
          "Connecting to <{}/{}>",
          state.spool.host, state.spool.endpoint
        )
        .as_str(),
        LogLevel::Info,
        false,
      )
      .await;

    if let Some(mut write) = self.write.lock().await.take() {
      write.close().await.ok();
    }
    state.flags |= Flags::CONNECTING;

    let mut request = self
      .uri(state.spool.clone())
      .into_client_request()
      .expect("Invalid WebSocket URL");
    let protocol_header = HeaderValue::from_str(state.spool.token.as_str())
      .expect("Invalid characters in token/protocol");

    request
      .headers_mut()
      .insert(SEC_WEBSOCKET_PROTOCOL, protocol_header);

    // TODO: handle error for reconnect
    let (stream, _) = match connect_async(request).await {
      Ok(stream) => stream,
      Err(error) => {
        if !state.flags.contains(Flags::SUCCESSFUL) {
          self
            .log(
              &format!("Connection error: {}", error),
              LogLevel::Error,
              true,
            )
            .await;
        }

        let mut ribbon = self.clone();

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

    state.flags |= Flags::ALIVE | Flags::SUCCESSFUL;
    state.flags &= !Flags::TIMING_OUT;

    self.write = Arc::new(Mutex::new(Some(write)));

    drop(state);

    let session = self.state.lock().await.session.clone();

    if session.token_id.is_empty() {
      self.pipe("new", serde_json::json!(null)).await;
    } else {
      self
        .pipe(
          "session",
          serde_json::json!({
            "ribbonid": session.ribbon_id,
            "tokenid": session.token_id,
          }),
        )
        .await;
    }

    let ribbon = self.clone();
    tokio::spawn(Box::pin(async move {
      Self::listen(read, ribbon).await;
    }));

    Ok(())
  }

  async fn pipe(&mut self, command: &str, data: serde_json::Value) {
    self.emitter.lock().await.emit_raw(
      "client.ribbon.send",
      serde_json::json!({
        "command": command,
        "data": data,
      }),
    );
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

    let packet = self.encode(command, data).await;

    match packet {
      TransportData::UTF8(s) => {
        if let Some(write) = &mut *self.write.lock().await {
          let _ = write.send(Message::Text(s.into()));
        }
      }

      TransportData::Binary(_b) => {
        unimplemented!()
      }
    }
  }

  pub async fn emit<T: Event>(&mut self, event: T) {
    if T::NAME.starts_with("client.") {
      self.emitter.lock().await.emit(event);
    } else {
      self
        .pipe(
          T::NAME,
          serde_json::to_value(&event).unwrap_or(serde_json::json!({})),
        )
        .await;
    }
  }

  pub async fn emit_raw(&mut self, command: &str, data: serde_json::Value) {
    if command.starts_with("client.") {
      self.emitter.lock().await.emit_raw(command, data);
    } else {
      self.pipe(command, data).await;
    }
  }

  async fn process_message(&mut self, msg: serde_json::Value) {
    let command = msg["command"].as_str().unwrap_or("");
    let data = &msg["data"];
    let id = msg["id"].as_u64().map(|v| v as u32);

    if let Some(id) = id {
      let received_id = self.state.lock().await.received_id;
      if id > received_id {
        let packet = InPacket {
          id: Some(id),
          command: command.to_string(),
          data: data.clone(),
        };
        if id == received_id + 1 {
          self.run_message(packet).await;
        } else {
          self.state.lock().await.recv_queue.push(packet);
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
  }

  async fn process_queue(&mut self) {
    let mut state = self.state.lock().await;
    if state.recv_queue.is_empty() {
      return;
    }

    if state.recv_queue.len() > CACHE_MAXSIZE {
      // TODO: close "too many lost packets"
    }

    state.recv_queue.sort_by_key(|p| p.id.unwrap_or(0));

    let mut packets = Vec::new();

    while let Some(packet) = state.recv_queue.first() {
      if let Some(id) = packet.id {
        if id <= state.received_id {
          continue;
        } else if id != state.received_id + 1 {
          break;
        } else {
          packets.push(packet.clone());
          state.received_id = id;
        }
      } else {
        state.recv_queue.remove(0);
      }
    }

    drop(state);

    for packet in packets {
      self.run_message(packet).await;
    }
  }

  async fn run_message(&mut self, packet: InPacket) {
    if packet.command != "ping" && packet.command != "packets" {
      self.emitter.lock().await.emit_raw(
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
        let mut state = self.state.lock().await;
        let config = self.config.lock().await.clone();

        let ribbonid = packet.data["ribbonid"].as_str().unwrap_or("").to_string();
        let tokenid = packet.data["tokenid"].as_str().unwrap_or("").to_string();

        state.flags &= !Flags::CONNECTING;

        state.session.ribbon_id = ribbonid;

        let session = state.session.clone();
        let spool = state.spool.clone();
        let sent_queue = state.sent_queue.clone();

        drop(state);

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
            .await;
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
            .await;
        }

        let mut state = self.state.lock().await;

        state.session.token_id = tokenid;
      }

      "ping" => {
        let id = packet.data["recvid"].as_u64().map(|v| v as u32);
        let mut state = self.state.lock().await;

        state.pinger.time = Instant::now() - state.pinger.last;

        if let Some(id) = id {
          while state.sent_queue.len() > 0
            && state.sent_queue[0].id.map(|i| i <= id).unwrap_or(false)
          {
            state.sent_queue.remove(0);
          }
        }
      }

      "kick" => {
        let reason = packet.data["reason"]
          .as_str()
          .unwrap_or("unknown")
          .to_string();

        self.state.lock().await.last_disconnect_reason = "server closed ribbon".into();

        self
          .log(&format!("kicked: {}", reason), LogLevel::Error, true)
          .await;

        self.close("");
      }

      "nope" => {
        let reason = packet.data["reason"]
          .as_str()
          .unwrap_or("unknown")
          .to_string();

        self.state.lock().await.last_disconnect_reason = reason.clone();

        self
          .log(
            &format!("packet rejected: {}", reason),
            LogLevel::Warning,
            true,
          )
          .await;

        self.close("");
      }

      "server.authorize" => {
        let data = serde_json::from_value::<events::recv::server::Authorize>(packet.data.clone());

        match data {
          Ok(data) => {
            let spool = self.state.lock().await.spool.clone();
            if data.success {
              self.log("Authorized", LogLevel::Info, false).await;

              self
                .emit(events::send::social::Presence {
                  status: crate::types::social::Status::Online,
                  detail: crate::types::social::Detail::Menus,
                })
                .await;

              self
                .emit(events::send::client::Ready {
                  endpoint: self.uri(spool),
                  social: data.social,
                })
                .await;

              // TODO: self report
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
      .lock()
      .await
      .emit_raw(packet.command.as_str(), packet.data.clone());
  }

  async fn switch(&mut self, target: &str) {
    {
      let mut state = self.state.lock().await;

      state.spool.endpoint = target.to_string();
      state.flags |= Flags::CONNECTING;
    }

    sleep(Duration::from_millis(5)).await;

    self.__internal_reconnect().await;
  }

  fn __internal_reconnect(
    &mut self,
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

      let flags = self.state.lock().await.flags;

      if flags.contains(Flags::DEAD) {
        let mut ribbon = self.clone();
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

  async fn reconnect(&mut self) {
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

    let flags = self.state.lock().await.flags;

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

    let mut ribbon = self.clone();

    reconnect_state
      .reconnect_handle
      .replace(tokio::spawn(Box::pin(async move {
        sleep(wait_time).await;
        ribbon.__internal_reconnect().await;
      })));

    reconnect_state.reconnect_penalty = 0;
    reconnect_state.reconnect_count += 1;
  }

  pub async fn close(&mut self, reason: &str) {
    let mut state = self.state.lock().await;
    if !reason.is_empty() {
      state.last_disconnect_reason = reason.to_string();
    }

    self.emitter.lock().await.emit(events::send::client::Close {
      reason: state.last_disconnect_reason.clone(),
    });

    drop(state);

    if self.write.lock().await.is_some() {
      self.emit(events::send::Die {}).await;
      self.write.lock().await.take().map(|mut write| async move {
        write.close().await.ok();
      });
    }

    let mut state = self.state.lock().await;

    state.flags |= Flags::DEAD;

    state.pinger.handle.take().map(|h| h.abort());
    self
      .reconnect_state
      .lock()
      .await
      .reconnect_handle
      .take()
      .map(|h| h.abort());
  }

  async fn listen(
    mut stream: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    mut ribbon: Ribbon,
  ) {
    while let Some(message) = stream.next().await {
      match message {
        Ok(msg) => {
          match msg {
            Message::Close(frame) => {
              let code = frame.as_ref().map(|f| f.code.into()).unwrap_or(0);
              let reason = close_code_reason(code);
              ribbon.state.lock().await.last_disconnect_reason = reason.into();
              ribbon.state.lock().await.flags |= Flags::CONNECTING;
              ribbon.reconnect().await;
              return;
            }
            _ => {}
          }

          let decoded = ribbon.config.lock().await.transport.decode(match msg {
            Message::Text(ref s) => s.as_bytes(),
            Message::Binary(ref b) => b,
            _ => continue, // ignore other message types
          });

          ribbon.process_message(decoded).await;
          ribbon.process_queue().await;
        }
        Err(e) => {
          match e {
            Error::ConnectionClosed => {}

            _ => {}
          }
          // handle error
        }
      }
    }
  }

  pub async fn wait<T: Event>(&mut self) -> Option<T> {
    let emitter = self.emitter.lock().await.clone();
    emitter.once::<T>().await
  }

  pub async fn on<T: Event>(&mut self, callback: impl Fn(T) + Send + Sync + 'static) {
    let emitter = self.emitter.lock().await;
    emitter.on(callback).await;
  }

  pub async fn once<T: Event>(&mut self, callback: impl Fn(T) + Send + Sync + 'static) {
    let emitter = self.emitter.lock().await.clone();
    tokio::spawn(Box::pin(async move {
      emitter.once::<T>().await.map(callback);
    }));
  }
}
