use crate::{
    dominio::{ESCRITURA, ServerConfig, UniversoMcp, herramientas},
    transporte::Acotado,
    worker,
};
use rmcp::{ErrorData, RoleServer, ServerHandler, ServiceExt, model::*, service::RequestContext};
use serde_json::{Map, Value, json};
use std::{borrow::Cow, sync::Arc};

/// Nombre reservado: `search` con `universe: "todos"` busca en todos.
const TODOS: &str = "todos";

struct Servidor {
    /// Universo por defecto (o el único, con rutas explícitas).
    config: ServerConfig,
    /// Con `--criterio`: cada universo del repositorio; vacío con rutas.
    universos: Vec<UniversoMcp>,
    por_defecto: Option<String>,
    ocupado: Arc<tokio::sync::Semaphore>,
}

impl Servidor {
    fn escritura(&self) -> bool {
        self.universos.iter().any(|u| u.escritura)
    }

    /// Universos sobre los que corre la herramienta, con su nombre si el
    /// servidor tiene varios.
    fn destinos(
        &self,
        herramienta: &str,
        universo: Option<Value>,
    ) -> Result<Vec<(Option<String>, ServerConfig)>, &'static str> {
        let escribe = ESCRITURA.contains(&herramienta);
        if self.universos.is_empty() {
            return match universo {
                None => Ok(vec![(None, self.config.clone())]),
                Some(_) => Err("invalid_tool_arguments"),
            };
        }
        let nombre = match universo {
            None => self.por_defecto.clone().unwrap_or_default(),
            Some(Value::String(nombre)) => nombre,
            Some(_) => return Err("invalid_tool_arguments"),
        };
        if nombre == TODOS {
            if herramienta != "search" {
                return Err("invalid_tool_arguments");
            }
            return Ok(self
                .universos
                .iter()
                .map(|u| (Some(u.nombre.clone()), u.config.clone()))
                .collect());
        }
        let universo = self
            .universos
            .iter()
            .find(|u| u.nombre == nombre)
            .ok_or("universe_not_found")?;
        if escribe && !universo.escritura {
            return Err("universe_read_only");
        }
        Ok(vec![(
            Some(universo.nombre.clone()),
            universo.config.clone(),
        )])
    }
}

fn error(code: &str) -> (Value, bool) {
    (
        json!({"schema":"felix-mcp-v1","status":"error","code":code}),
        true,
    )
}

/// Una llamada a la vez: si el worker está ocupado, `server_busy`.
fn turno(
    ocupado: &Arc<tokio::sync::Semaphore>,
    herramienta: &str,
    universo: Option<&str>,
) -> Result<tokio::sync::OwnedSemaphorePermit, ErrorData> {
    Arc::clone(ocupado).try_acquire_owned().map_err(|_| {
        crate::uso::anotar(herramienta, universo, false);
        ErrorData::internal_error("server_busy", None)
    })
}

/// El contenido textual de una respuesta; `response_invalid` si no se puede serializar.
fn texto<T: serde::Serialize>(valor: &T) -> Result<String, ErrorData> {
    serde_json::to_string(valor).map_err(|_| ErrorData::internal_error("response_invalid", None))
}

/// Las definiciones de `tools/list`; `tool_schema_invalid` si alguna no es una `Tool`.
fn a_tools(definiciones: Vec<Value>) -> Result<Vec<Tool>, ErrorData> {
    definiciones
        .into_iter()
        .map(serde_json::from_value)
        .collect::<Result<Vec<Tool>, _>>()
        .map_err(|_| ErrorData::internal_error("tool_schema_invalid", None))
}

fn respuesta(resultado: Result<worker::Respuesta, &'static str>) -> (Value, bool) {
    match resultado {
        Ok(worker::Respuesta {
            resultado: Some(value),
            error: None,
        }) => (value, false),
        Ok(worker::Respuesta {
            error: Some(code), ..
        }) => error(&code),
        Err(code) => error(code),
        _ => error("worker_failed"),
    }
}

/// `search` en todos los universos: une los resultados por puntaje, con el
/// universo de cada uno, y respeta `limit`.
fn unir_busquedas(resultados: Vec<(Option<String>, Value)>, limite: usize) -> Value {
    let mut total = 0;
    let mut coincidencias = Vec::new();
    for (universo, valor) in resultados {
        total += valor["summary"]["total"].as_u64().unwrap_or(0);
        for mut m in valor["matches"].as_array().cloned().unwrap_or_default() {
            m["universe"] = json!(universo);
            coincidencias.push(m);
        }
    }
    coincidencias.sort_by(|a, b| {
        b["score"]
            .as_u64()
            .cmp(&a["score"].as_u64())
            .then_with(|| a["universe"].as_str().cmp(&b["universe"].as_str()))
            .then_with(|| a["path"].as_str().cmp(&b["path"].as_str()))
    });
    coincidencias.truncate(limite);
    json!({"schema":"felix-mcp-v1","tool":"search","status":"ok",
           "summary":{"total":total,"returned":coincidencias.len()},
           "values_included":true,"matches":coincidencias,"persisted":false})
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
        let mut definiciones = herramientas(!self.universos.is_empty(), self.escritura());
        if crate::metricas::activa() {
            definiciones.push(crate::metricas::definicion());
        }
        let tools = a_tools(definiciones)?;
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
        let nombre = request.name.into_owned();
        if nombre == "metrics" && crate::metricas::activa() {
            let argumentos: Map<String, Value> = request.arguments.unwrap_or_default();
            // Mismo límite de concurrencia que las demás herramientas.
            let permit = turno(&self.ocupado, "metrics", None)?;
            let (value, error) = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                crate::metricas::ejecutar(&argumentos)
            })
            .await
            .unwrap_or_else(|_| {
                (
                    json!({"schema":"felix-mcp-v1","status":"error","code":"worker_failed"}),
                    true,
                )
            });
            crate::uso::anotar("metrics", None, !error);
            let text = texto(&value)?;
            let mut result = if error {
                CallToolResult::error(vec![ContentBlock::text(text)])
            } else {
                CallToolResult::success(vec![ContentBlock::text(text)])
            };
            result.structured_content = Some(value);
            return Ok(result.into());
        }
        let consulta = matches!(
            nombre.as_str(),
            "context" | "propose" | "check" | "why" | "search"
        );
        let escritura = ESCRITURA.contains(&nombre.as_str()) && self.escritura();
        if !(consulta || escritura) {
            crate::uso::anotar("desconocida", None, false);
            return Err(ErrorData::invalid_params("unknown_tool", None));
        }
        let mut argumentos: Map<String, Value> = request.arguments.unwrap_or_default();
        let universo = argumentos.remove("universe");
        let herramienta = nombre.clone();
        // Sólo un universo conocido (o «todos») se anota tal cual: el
        // argumento es texto libre y el registro no guarda contenido.
        let universo_pedido = universo.as_ref().and_then(Value::as_str).map(|u| {
            if u == "todos" || self.universos.iter().any(|x| x.nombre == u) {
                u.to_string()
            } else {
                "otro".to_string()
            }
        });
        // Como en el worker: 5.0 es un entero válido en JSON Schema.
        let limite = argumentos
            .get("limit")
            .and_then(|v| {
                v.as_u64().or_else(|| {
                    v.as_f64()
                        .filter(|f| f.fract() == 0.0 && *f >= 0.0)
                        .map(|f| f as u64)
                })
            })
            .unwrap_or(20) as usize;
        let (value, error) = match self.destinos(&nombre, universo) {
            Err(code) => error(code),
            Ok(destinos) => {
                let permit = turno(&self.ocupado, &herramienta, universo_pedido.as_deref())?;
                let varios = destinos.len() > 1;
                let con_nombre = !self.universos.is_empty();
                let corrida = tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    destinos
                        .into_iter()
                        .map(|(universo, config)| {
                            let peticion = worker::Peticion {
                                nombre: nombre.clone(),
                                argumentos: Value::Object(argumentos.clone()),
                            };
                            (universo, worker::consultar(config, peticion))
                        })
                        .collect::<Vec<_>>()
                })
                .await;
                match corrida {
                    Err(_) => error("worker_failed"),
                    Ok(resultados) => {
                        let mut bien = Vec::new();
                        let mut fallo = None;
                        for (universo, resultado) in resultados {
                            match respuesta(resultado) {
                                (value, false) => bien.push((universo, value)),
                                (mut value, true) => {
                                    // Con varios universos, decir cuál falló.
                                    if con_nombre {
                                        value["universe"] = json!(universo);
                                    }
                                    fallo.get_or_insert((value, true));
                                }
                            }
                        }
                        match fallo {
                            Some(otro) => otro,
                            None if varios => (unir_busquedas(bien, limite), false),
                            None => {
                                let (universo, mut value) =
                                    bien.pop().unwrap_or((None, Value::Null));
                                if con_nombre {
                                    value["universe"] = json!(universo);
                                }
                                (value, false)
                            }
                        }
                    }
                }
            }
        };
        crate::uso::anotar(&herramienta, universo_pedido.as_deref(), !error);
        let text = texto(&value)?;
        let mut result = if error {
            CallToolResult::error(vec![ContentBlock::text(text)])
        } else {
            CallToolResult::success(vec![ContentBlock::text(text)])
        };
        result.structured_content = Some(value);
        Ok(result.into())
    }
}

/// Sirve por stdio. `universos` vacío: rutas explícitas (`config`); con
/// `--criterio`, cada universo y el nombre del por defecto.
pub async fn servir(
    config: ServerConfig,
    universos: Vec<UniversoMcp>,
    por_defecto: Option<String>,
) -> Result<(), ()> {
    let server = Servidor {
        config,
        universos,
        por_defecto,
        ocupado: Arc::new(tokio::sync::Semaphore::new(1)),
    };
    let service = server
        .serve(Acotado::new(tokio::io::stdin(), tokio::io::stdout()))
        .await
        .map_err(|_| ())?;
    service.waiting().await.map_err(|_| ())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn una_segunda_llamada_concurrente_es_server_busy() {
        let ocupado = Arc::new(tokio::sync::Semaphore::new(1));
        let primera = turno(&ocupado, "check", None).unwrap();
        let error = turno(&ocupado, "check", None).unwrap_err();
        assert_eq!(error.message, "server_busy");
        drop(primera);
        assert!(turno(&ocupado, "check", None).is_ok());
    }

    #[test]
    fn una_respuesta_que_no_se_serializa_es_response_invalid() {
        struct Rota;
        impl serde::Serialize for Rota {
            fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
                Err(serde::ser::Error::custom("rota"))
            }
        }
        assert_eq!(texto(&Rota).unwrap_err().message, "response_invalid");
        assert_eq!(texto(&json!({"a": 1})).unwrap(), r#"{"a":1}"#);
    }

    #[test]
    fn las_definiciones_anunciadas_son_tools_validas() {
        for (criterio, escritura) in [(false, false), (true, false), (true, true)] {
            let mut definiciones = herramientas(criterio, escritura);
            definiciones.push(crate::metricas::definicion());
            assert!(a_tools(definiciones).is_ok());
        }
        let rota = vec![json!({"name": 5})];
        assert_eq!(a_tools(rota).unwrap_err().message, "tool_schema_invalid");
    }
}
