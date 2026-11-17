//! Frontera stdio acotada. SDK conserva negociación y despacho MCP.
use crate::{MAX_FRAME_BYTES, MAX_JSON_DEPTH, MAX_RESPONSE_BYTES};
use rmcp::{
    RoleServer, model::ClientJsonRpcMessage, service::TxJsonRpcMessage, transport::Transport,
};
use serde_json::{Value, json};
use std::{io, sync::Arc};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader},
    sync::Mutex,
};

pub(crate) struct Acotado<R, W> {
    read: BufReader<R>,
    write: Arc<Mutex<W>>,
    pending: Vec<u8>,
    fase: u8,
}
impl<R: AsyncRead + Unpin, W: AsyncWrite + Unpin + Send + 'static> Acotado<R, W> {
    pub(crate) fn new(read: R, write: W) -> Self {
        Self {
            read: BufReader::new(read),
            write: Arc::new(Mutex::new(write)),
            pending: Vec::new(),
            fase: 0,
        }
    }
    async fn error(&mut self, id: Value, code: i64, message: &'static str) -> io::Result<()> {
        self.responder(json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}}))
            .await
    }
    async fn responder(&mut self, value: Value) -> io::Result<()> {
        // Una cancelación de receive no interrumpe una escritura parcial.
        let writer = Arc::clone(&self.write);
        tokio::spawn(async move { escribir(&writer, value).await })
            .await
            .map_err(io::Error::other)?
    }
}
async fn escribir<W: AsyncWrite + Unpin>(writer: &Arc<Mutex<W>>, value: Value) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(&value).map_err(io::Error::other)?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        bytes = serde_json::to_vec(&json!({"jsonrpc":"2.0","id":value.get("id").cloned().unwrap_or(Value::Null),"error":{"code":-32603,"message":"response_limit"}})).map_err(io::Error::other)?;
        if bytes.len() > MAX_RESPONSE_BYTES {
            return Err(io::Error::other("response_limit"));
        }
    }
    bytes.push(b'\n');
    let mut writer = writer.lock().await;
    writer.write_all(&bytes).await?;
    writer.flush().await
}
impl<R, W> Transport<RoleServer> for Acotado<R, W>
where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send + 'static,
{
    type Error = io::Error;
    fn send(
        &mut self,
        item: TxJsonRpcMessage<RoleServer>,
    ) -> impl Future<Output = io::Result<()>> + Send + 'static {
        let writer = Arc::clone(&self.write);
        async move {
            escribir(
                &writer,
                serde_json::to_value(item).map_err(io::Error::other)?,
            )
            .await
        }
    }
    async fn receive(&mut self) -> Option<ClientJsonRpcMessage> {
        loop {
            // fill_buf/consume y pending persistentes soportan cancelación del SDK.
            let buffer = self.read.fill_buf().await.ok()?;
            if buffer.is_empty() {
                if !self.pending.is_empty() {
                    let _ = self.error(Value::Null, -32700, "parse_error").await;
                }
                return None;
            }
            let count = buffer
                .iter()
                .position(|b| *b == b'\n')
                .map_or(buffer.len(), |i| i + 1);
            if self.pending.len().saturating_add(count) > MAX_FRAME_BYTES + 1 {
                let _ = self.error(Value::Null, -32600, "message_limit").await;
                return None;
            }
            self.pending.extend_from_slice(&buffer[..count]);
            self.read.consume(count);
            if self.pending.last() != Some(&b'\n') {
                continue;
            }
            let mut line = std::mem::take(&mut self.pending);
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if !depth_permitted(&line) {
                self.error(Value::Null, -32600, "json_depth_limit")
                    .await
                    .ok()?;
                continue;
            }
            let value: Value = match serde_json::from_slice(&line) {
                Ok(value) => value,
                Err(_) => {
                    self.error(Value::Null, -32700, "parse_error").await.ok()?;
                    continue;
                }
            };
            let id = value
                .get("id")
                .filter(|v| v.is_i64() || v.is_u64() || v.is_string())
                .cloned()
                .unwrap_or(Value::Null);
            if !value.is_object()
                || value.get("jsonrpc") != Some(&json!("2.0"))
                || value.get("method").and_then(Value::as_str).is_none()
                || value.get("id").is_some_and(|_| id.is_null())
            {
                self.error(id, -32600, "invalid_request").await.ok()?;
                continue;
            }
            let method = value["method"].as_str().unwrap();
            let notification = !value.as_object().unwrap().contains_key("id");
            if notification {
                if method == "notifications/initialized" && self.fase == 1 {
                    if let Ok(parsed) = serde_json::from_value(value) {
                        self.fase = 2;
                        return Some(parsed);
                    }
                }
                // No respuestas a notifications. No habilitan lifecycle implícito.
                continue;
            }
            if method == "ping" && self.fase != 2 {
                if value.get("params").is_some_and(|p| !p.is_object()) {
                    self.error(id, -32602, "invalid_params").await.ok()?;
                } else {
                    self.responder(json!({"jsonrpc":"2.0","id":id,"result":{}}))
                        .await
                        .ok()?;
                }
                continue;
            }
            if method == "initialize" {
                if self.fase != 0 {
                    self.error(id, -32600, "already_initialized").await.ok()?;
                    continue;
                }
            } else if self.fase != 2 {
                self.error(id, -32000, "not_initialized").await.ok()?;
                continue;
            }
            if !matches!(method, "initialize" | "ping" | "tools/list" | "tools/call") {
                self.error(id, -32601, "method_not_found").await.ok()?;
                continue;
            }
            let params = value.get("params").cloned().unwrap_or(Value::Null);
            let valid = match method {
                "initialize" => {
                    serde_json::from_value::<rmcp::model::InitializeRequestParams>(params).is_ok()
                }
                "tools/call" => {
                    serde_json::from_value::<rmcp::model::CallToolRequestParams>(params).is_ok()
                }
                "tools/list" => {
                    params.is_null()
                        || serde_json::from_value::<rmcp::model::PaginatedRequestParams>(params)
                            .is_ok()
                }
                _ => params.is_null() || params.is_object(),
            };
            if !valid {
                self.error(id, -32602, "invalid_params").await.ok()?;
                continue;
            }
            let initialize = method == "initialize";
            match serde_json::from_value(value) {
                Ok(parsed) => {
                    if initialize {
                        self.fase = 1;
                    }
                    return Some(parsed);
                }
                Err(_) => {
                    self.error(id, -32602, "invalid_params").await.ok()?;
                }
            }
        }
    }
    async fn close(&mut self) -> io::Result<()> {
        self.write.lock().await.flush().await
    }
}
pub(crate) fn depth_permitted(bytes: &[u8]) -> bool {
    let (mut depth, mut string, mut escaped) = (0usize, false, false);
    for &b in bytes {
        if string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                string = false;
            }
        } else {
            match b {
                b'"' => string = true,
                b'{' | b'[' => {
                    depth += 1;
                    if depth > MAX_JSON_DEPTH {
                        return false;
                    }
                }
                b'}' | b']' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, duplex};

    async fn exchange(input: Vec<u8>, fragmented: bool) -> Vec<Value> {
        let (mut client, server) = duplex(73);
        let (reader, writer) = tokio::io::split(server);
        let task = tokio::spawn(async move {
            let mut transport = Acotado::new(reader, writer);
            while transport.receive().await.is_some() {}
        });
        let (mut read, mut write) = tokio::io::split(&mut client);
        let send = async move {
            for chunk in input.chunks(if fragmented { 7 } else { input.len().max(1) }) {
                write.write_all(chunk).await.unwrap();
            }
            write.shutdown().await.unwrap();
        };
        let receive = async {
            let mut output = Vec::new();
            read.read_to_end(&mut output).await.unwrap();
            output
                .split(|b| *b == b'\n')
                .filter(|line| !line.is_empty())
                .map(|line| serde_json::from_slice(line).unwrap())
                .collect()
        };
        let (_, output) = tokio::join!(send, receive);
        task.await.unwrap();
        output
    }

    #[tokio::test]
    async fn ping_fragmentado_antes_initialize_y_notificaciones_sin_respuesta() {
        let output = exchange(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"}\n{\"jsonrpc\":\"2.0\",\"id\":8,\"method\":\"tools/list\"}\n".to_vec(), true).await;
        assert_eq!(output.len(), 2);
        assert_eq!(output[0], json!({"jsonrpc":"2.0","id":7,"result":{}}));
        assert_eq!(output[1]["error"]["message"], "not_initialized");
    }

    #[tokio::test]
    async fn errores_redactados_y_limite_de_entrada() {
        let output = exchange(b"secret_invalid_json\n[]\n".to_vec(), true).await;
        assert_eq!(output[0]["error"]["code"], -32700);
        assert_eq!(output[1]["error"]["code"], -32600);
        assert!(!serde_json::to_string(&output).unwrap().contains("secret"));
        let mut frame = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}".to_vec();
        frame.resize(MAX_FRAME_BYTES, b' ');
        frame.push(b'\n');
        assert!(exchange(frame, false).await[0].get("result").is_some());
    }

    #[tokio::test]
    async fn entrada_excesiva_cierra_con_error_completo() {
        let (mut reader, writer) = duplex(128);
        let task = tokio::spawn(async move {
            let input = vec![b' '; MAX_FRAME_BYTES + 2];
            let mut transport = Acotado::new(input.as_slice(), writer);
            assert!(transport.receive().await.is_none());
        });
        let mut output = Vec::new();
        reader.read_to_end(&mut output).await.unwrap();
        task.await.unwrap();
        let result: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(result["error"]["message"], "message_limit");
    }

    #[tokio::test]
    async fn envelope_invalido_y_metodo_desconocido_separados() {
        let (mut reader, writer) = duplex(128);
        let task = tokio::spawn(async move {
            let input = concat!(
                "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"check\",\"arguments\":[]}}\n",
                "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"unknown\"}\n"
            );
            let mut transport = Acotado::new(input.as_bytes(), writer);
            transport.fase = 2;
            assert!(transport.receive().await.is_none());
        });
        let mut output = Vec::new();
        reader.read_to_end(&mut output).await.unwrap();
        task.await.unwrap();
        let replies: Vec<Value> = output
            .split(|b| *b == b'\n')
            .filter(|s| !s.is_empty())
            .map(|line| serde_json::from_slice(line).unwrap())
            .collect();
        assert_eq!(replies[0]["error"]["code"], -32602);
        assert_eq!(replies[1]["error"]["code"], -32601);
    }

    #[tokio::test]
    async fn overflow_emite_error_json_completo() {
        let (mut reader, writer) = duplex(128);
        let writer = Arc::new(Mutex::new(writer));
        let task = tokio::spawn(async move {
            escribir(
                &writer,
                json!({"jsonrpc":"2.0","id":91,"result":"x".repeat(MAX_RESPONSE_BYTES)}),
            )
            .await
            .unwrap();
        });
        let mut output = Vec::new();
        reader.read_to_end(&mut output).await.unwrap();
        task.await.unwrap();
        let result: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(result["id"], 91);
        assert_eq!(result["error"]["message"], "response_limit");
    }

    #[test]
    fn profundidad_cuenta_estructura_no_cadenas() {
        assert!(depth_permitted(&[b'['; MAX_JSON_DEPTH]));
        assert!(!depth_permitted(&[b'['; MAX_JSON_DEPTH + 1]));
        assert!(depth_permitted(br#"{"s":"[[[\\\"[[["}"#));
    }
}
