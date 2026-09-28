//! JSONL Unix-socket client. One request → one response; server events
//! on a non-subscribed connection are skipped by `call`.

use serde_json::{json, Value};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::UnixStream;

use signaltty_proto::{Request, Response};

#[derive(Debug, Error)]
pub enum CliError {
    #[error("cannot connect to server at {0}: {1}")]
    Connect(String, String),
    #[error("connection lost: {0}")]
    Io(String),
    #[error("server error {code}: {message}")]
    Server { code: String, message: String },
    #[error("{0}")]
    Usage(String),
}

pub struct Client {
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
    next_id: u64,
}

impl Client {
    pub async fn connect(socket: &std::path::Path) -> Result<Client, CliError> {
        let stream = UnixStream::connect(socket)
            .await
            .map_err(|e| CliError::Connect(socket.display().to_string(), e.to_string()))?;
        let (read_half, writer) = stream.into_split();
        Ok(Client {
            reader: BufReader::new(read_half),
            writer,
            next_id: 1,
        })
    }

    pub async fn call(&mut self, method: &str, params: Value) -> Result<Value, CliError> {
        let id = self.next_id.to_string();
        self.next_id += 1;
        let req = Request {
            protocol: signaltty_proto::PROTOCOL.to_string(),
            id,
            method: method.to_string(),
            params: if params.is_null() { json!({}) } else { params },
        };
        let mut line = serde_json::to_string(&req).unwrap();
        line.push('\n');
        self.writer
            .write_all(line.as_bytes())
            .await
            .map_err(|e| CliError::Io(e.to_string()))?;
        loop {
            let mut buf = String::new();
            let n = self
                .reader
                .read_line(&mut buf)
                .await
                .map_err(|e| CliError::Io(e.to_string()))?;
            if n == 0 {
                return Err(CliError::Io("server closed connection".to_string()));
            }
            let v: Value =
                serde_json::from_str(buf.trim()).map_err(|e| CliError::Io(e.to_string()))?;
            if v.get("type").and_then(|t| t.as_str()) == Some("event") {
                continue; // skip stray events on call connections
            }
            let resp: Response =
                serde_json::from_value(v).map_err(|e| CliError::Io(e.to_string()))?;
            if resp.ok {
                return Ok(resp.result);
            }
            let e = resp.error.unwrap_or(signaltty_proto::ErrorBody {
                code: "INTERNAL".to_string(),
                message: "unknown error".to_string(),
                details: Value::Null,
            });
            return Err(CliError::Server {
                code: e.code,
                message: e.message,
            });
        }
    }

    pub fn into_parts(self) -> (BufReader<OwnedReadHalf>, OwnedWriteHalf) {
        (self.reader, self.writer)
    }
}
