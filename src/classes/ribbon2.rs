use std::{sync::Arc, time::Duration};

use futures_util::StreamExt;
use http::header::{HeaderValue, SEC_WEBSOCKET_PROTOCOL};
use tokio_tungstenite::{
  connect_async,
  tungstenite::{client::IntoClientRequest, http},
};

use crate::{
  classes::ribbon2::codec::CodecTrait,
  types::{game::Handling, server, user::Me},
  utils::{
    EventEmitter, Logger,
    api::{self, Api},
  },
};
use bitflags::bitflags;
use tokio::{sync::mpsc, time::Instant};

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

#[derive(Debug, Clone)]
pub struct Pinger {
  pub heartbeat: u64,
  // something to stop it
  pub last: u64,
  pub time: u64,
}

#[derive(Debug, Clone)]
pub struct Session {
  pub token_id: String,
  pub session_id: String,
}

#[derive(Debug, Clone)]
pub struct OutPacket {
  pub id: Option<u32>,
  pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct InPacket {
  pub id: Option<u32>,
  pub comman: String,
  pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Options {
  pub logging: LoggingLevel,
  pub spooling: bool,
  pub debug: bool,
}

mod codec {
  use serde_json::json;

  pub trait CodecTrait {
    fn transport(&self) -> String;
    fn encode(&self, command: String, data: serde_json::Value) -> Vec<u8>;
    fn decode(&self, data: &[u8]) -> serde_json::Value;
  }

  #[derive(Debug, Clone)]
  pub struct JSON;

  impl CodecTrait for JSON {
    fn transport(&self) -> String {
      "json".to_string()
    }

    fn encode(&self, command: String, data: serde_json::Value) -> Vec<u8> {
      let json_str = json!({
        "command": command,
        "data": data,
      })
      .to_string();
      json_str.into_bytes()
    }

    fn decode(&self, data: &[u8]) -> serde_json::Value {
      let json_str = String::from_utf8_lossy(data);
      serde_json::from_str(&json_str)
        .unwrap_or_else(|_| panic!("Failed to decode JSON: {}\nRaw data: {:?}", json_str, data))
    }
  }

  #[derive(Debug, Clone)]
  pub enum Codec {
    JSON(JSON),
  }

  impl CodecTrait for Codec {
    fn transport(&self) -> String {
      match self {
        Codec::JSON(codec) => codec.transport(),
      }
    }

    fn encode(&self, command: String, data: serde_json::Value) -> Vec<u8> {
      match self {
        Codec::JSON(codec) => codec.encode(command, data),
      }
    }

    fn decode(&self, data: &[u8]) -> serde_json::Value {
      match self {
        Codec::JSON(codec) => codec.decode(data),
      }
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transport {
  JSON,
}

impl Transport {
  fn codec(&self) -> codec::Codec {
    match self {
      Transport::JSON => codec::Codec::JSON(codec::JSON),
    }
  }
}

pub struct Ribbon {
  write: mpsc::UnboundedSender<OutMsg>,
  token: String,
  handling: Handling,
  user_agent: String,
  codec: codec::Codec,
  spool: Spool,
  api: Api,

  me: Me,

  pinger: Pinger,
  session: Session,

  sent_id: u32,
  received_id: u32,

  flags: Flags,

  last_disconnect_reason: String,

  sent_queue: Vec<OutPacket>,
  recv_queue: Vec<InPacket>,

  last_reconnect: Instant,
  reconnect_count: u32,
  reconnect_penalty: u32,
  // reconnect_timeout for cancelling
  options: Options,

  pub emitter: Arc<EventEmitter>,
}

#[derive(Debug, Clone)]
pub struct Params {
  pub options: Options,
  pub token: String,
  pub handling: Handling,
  pub user_agent: String,
  pub transport: Transport,
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
      write: mpsc::unbounded_channel().0,
      token: params.token,
      handling: params.handling,
      user_agent: params.user_agent,
      codec: params.transport.codec(),
      spool: Spool {
        host: "".to_string(),
        endpoint: "".to_string(),
        token: "".to_string(),
        signature: env.signature,
      },
      api,

      me,

      pinger: Pinger {
        heartbeat: 0,
        last: 0,
        time: 0,
      },
      session: Session {
        token_id: String::new(),
        session_id: String::new(),
      },

      sent_id: 0,
      received_id: 0,

      flags: Flags::empty(),

      last_disconnect_reason: String::new(),

      sent_queue: Vec::new(),
      recv_queue: Vec::new(),

      last_reconnect: Instant::now(),
      reconnect_count: 0,
      reconnect_penalty: 0,

      options: params.options,

      emitter: Arc::new(EventEmitter::new()),
    })
  }

  pub fn log(&self, msg: &str, level: LogLevel, force: bool) {
    if level == LogLevel::Error {
      self.emitter.emit(
        "client.ribbon.error",
        serde_json::from_str(msg).unwrap_or(serde_json::json!({"error": msg})),
      );
    } else if level == LogLevel::Warning {
      self.emitter.emit(
        "client.ribbon.warn",
        serde_json::from_str(msg).unwrap_or(serde_json::json!({"warn": msg})),
      );
    } else {
      self.emitter.emit(
        "client.ribbon.log",
        serde_json::from_str(msg).unwrap_or(serde_json::json!({"log": msg})),
      );
    }

    if self.options.logging == LoggingLevel::None
      || (self.options.logging == LoggingLevel::Error && !force)
    {
      return;
    }

    match level {
      LogLevel::Info => println!("[Triangle.rs] {}", msg),
      LogLevel::Warning => eprintln!("[Triangle.rs] {}", msg),
      LogLevel::Error => eprintln!("[Triangle.rs] {}", msg),
    }
  }

  fn encode(&self, msg: String, data: serde_json::Value) -> Vec<u8> {
    let start = Instant::now();

    let res = self.codec.encode(msg, data);

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

  pub fn decode(&self, data: &[u8]) -> serde_json::Value {
    let start = Instant::now();

    let res = self.codec.decode(data);

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

  pub fn process_packet(&self, packet: Vec<u8>) -> serde_json::Value {
    self.decode(&packet)
  }

  pub fn uri(&self) -> String {
    format!("wss://{}/ribbon/{}", self.spool.host, self.spool.endpoint)
  }

  pub async fn connect(&mut self) -> Result<()> {
    let spool = self.api.server.spool(self.options.spooling).await?;

    self.spool = Spool {
      host: spool.host,
      endpoint: spool.endpoint,
      token: spool.token,
      signature: self.spool.signature.clone(),
    };

    self.log(
      format!(
        "Connecting to <{}/{}>",
        self.spool.host, self.spool.endpoint
      )
      .as_str(),
      LogLevel::Info,
      false,
    );

    // TODO: close any open socket

    self.flags |= Flags::CONNECTING;

    let mut request = self
      .uri()
      .into_client_request()
      .expect("Invalid WebSocket URL");
    let protocol_header = HeaderValue::from_str(self.spool.token.as_str())
      .expect("Invalid characters in token/protocol");

    request
      .headers_mut()
      .insert(SEC_WEBSOCKET_PROTOCOL, protocol_header);

    let (stream, _) = connect_async(request).await?;

		let (write, read) = stream.split();

		self.write = write;

    Ok(())
  }
}
