use crate::{
    dominio::{ServerConfig, herramientas},
    transporte::Acotado,
    worker,
};
use rmcp::{ErrorData, RoleServer, ServerHandler, ServiceExt, model::*, service::RequestContext};
use serde_json::{Value, json};
use std::{borrow::Cow, sync::Arc};

struct Servidor {
    config: ServerConfig,
    ocupado: Arc<tokio::sync::Semaphore>,
}
impl ServerHandler for Servidor {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_protocol_version(ProtocolVersion::V_2025_11_25)
    }
    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Owned(vec![ProtocolVersion::V_2025_11_25])
    }
    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        if request.is_some_and(|p| p.cursor.is_some()) {
            return Err(ErrorData::invalid_params("pagination_not_supported", None));
        }
        let tools = herramientas()
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<Vec<Tool>, _>>()
            .map_err(|_| ErrorData::internal_error("tool_schema_invalid", None))?;
        Ok(ListToolsResult {
            tools,
            ..Default::default()
        })
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        if !matches!(
            request.name.as_ref(),
            "context" | "propose" | "check" | "why" | "search"
        ) {
            return Err(ErrorData::invalid_params("unknown_tool", None));
        }
        let permit = Arc::clone(&self.ocupado)
            .try_acquire_owned()
            .map_err(|_| ErrorData::internal_error("server_busy", None))?;
        let config = ServerConfig {
            root: self.config.root.clone(),
            manifiesto: self.config.manifiesto.clone(),
            invariantes: self.config.invariantes.clone(),
        };
        let peticion = worker::Peticion {
            nombre: request.name.into_owned(),
            argumentos: Value::Object(request.arguments.unwrap_or_default()),
        };
        let result = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            worker::consultar(config, peticion)
        })
        .await;
        let (value, error) = match result {
            Ok(Ok(worker::Respuesta {
                resultado: Some(value),
                error: None,
            })) => (value, false),
            Ok(Ok(worker::Respuesta {
                error: Some(code), ..
            })) => (
                json!({"schema":"felix-mcp-v1","status":"error","code":code}),
                true,
            ),
            Ok(Err(code)) => (
                json!({"schema":"felix-mcp-v1","status":"error","code":code}),
                true,
            ),
            _ => (
                json!({"schema":"felix-mcp-v1","status":"error","code":"worker_failed"}),
                true,
            ),
        };
        let text = serde_json::to_string(&value)
            .map_err(|_| ErrorData::internal_error("response_invalid", None))?;
        let mut result = if error {
            CallToolResult::error(vec![ContentBlock::text(text)])
        } else {
            CallToolResult::success(vec![ContentBlock::text(text)])
        };
        result.structured_content = Some(value);
        Ok(result.into())
    }
}
pub async fn servir(config: ServerConfig) -> Result<(), ()> {
    let server = Servidor {
        config,
        ocupado: Arc::new(tokio::sync::Semaphore::new(1)),
    };
    let service = server
        .serve(Acotado::new(tokio::io::stdin(), tokio::io::stdout()))
        .await
        .map_err(|_| ())?;
    service.waiting().await.map_err(|_| ())?;
    Ok(())
}
