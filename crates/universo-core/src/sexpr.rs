//! Sintaxis textual de autoría de invariantes: S-expresiones con
//! vocabulario de SMT-LIB 2.6 (decisión resuelta del plan 02). El parser es
//! puramente sintáctico: resuelve variables contra los cuantificadores
//! envolventes y exige que toda constante de nodo o de arista lleve la
//! marca `@`. La validación contra el `Manifiesto` (sortes declaradas,
//! predicados, dominios) vive en `invariante`.

use std::collections::BTreeSet;

use crate::atomos::IdArista;
use crate::formula::PREFIJO_GENERADO;
use crate::formula::{
    DominioEsquema, Formula, Sentencia, SorteVar, SujetoTermino, TerminoArista, TerminoConstExt,
    TerminoEtiqueta, TerminoNodo, TerminoSorte, Var,
};
use crate::path::BundlePath;
use crate::tipos::{Etiqueta, Sorte};

/// Error de **autoría** (spec §4.3): un invariante mal escrito. Es un tipo
/// aparte de `ErrorEstructura` y de `ErrorAtomo` a propósito: un error de
/// autoría no es un error de datos, y el mensaje debe decir cuál es.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErrorAutoria {
    /// Línea del token que provocó el error, en base 1.
    pub linea: usize,
    /// Columna del token que provocó el error, en base 1.
    pub columna: usize,
    pub motivo: MotivoAutoria,
}

/// Motivo de un [`ErrorAutoria`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MotivoAutoria {
    ParentesisSinCerrar,
    ParentesisSobrante,
    CadenaSinCerrar,
    EscapeInvalido,
    TokenInesperado(String),
    FormaDesconocida(String),
    AridadIncorrecta {
        forma: String,
        esperada: String,
        encontrada: usize,
    },
    SorteDeVariableDesconocida(String),
    VariableRedeclarada(String),
    SimboloLibre(String),
    SorteEquivocadaEnPosicion {
        forma: String,
        posicion: usize,
    },
    TcSoloSobreParent(String),
    MetaFueraDeEsquema(String),
    MetaSinUso(String),
    DominioDeEsquemaDesconocido(String),
    ConstanteMalFormada {
        texto: String,
        motivo: String,
    },
    AnidamientoExcesivo {
        limite: usize,
    },
    NombreReservado(String),
    ColisionConMetavariable(String),
    TextoVacio,
    TextoSobrante,
}

impl std::fmt::Display for MotivoAutoria {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MotivoAutoria::ParentesisSinCerrar => write!(f, "falta cerrar un paréntesis"),
            MotivoAutoria::ParentesisSobrante => write!(f, "hay un paréntesis de cierre de más"),
            MotivoAutoria::CadenaSinCerrar => write!(f, "falta cerrar una cadena entre comillas"),
            MotivoAutoria::EscapeInvalido => write!(f, "escape inválido dentro de la cadena"),
            MotivoAutoria::TokenInesperado(t) => write!(f, "token inesperado {t:?}"),
            MotivoAutoria::FormaDesconocida(t) => write!(f, "forma desconocida {t:?}"),
            MotivoAutoria::AridadIncorrecta {
                forma,
                esperada,
                encontrada,
            } => write!(
                f,
                "la forma {forma:?} espera {esperada} argumentos y recibió {encontrada}"
            ),
            MotivoAutoria::SorteDeVariableDesconocida(s) => {
                write!(f, "la sorte de variable {s:?} no es N ni E")
            }
            MotivoAutoria::VariableRedeclarada(v) => {
                write!(f, "la variable {v:?} ya está ligada en este alcance")
            }
            MotivoAutoria::SimboloLibre(s) => write!(
                f,
                "el símbolo {s:?} no está ligado por ningún cuantificador; \
                 si querías una constante, escribila con la marca `@`"
            ),
            MotivoAutoria::SorteEquivocadaEnPosicion { forma, posicion } => write!(
                f,
                "el argumento {posicion} de {forma:?} es de la otra sorte"
            ),
            MotivoAutoria::TcSoloSobreParent(p) => write!(
                f,
                "`tc` solo se aplica sobre `Parent` en esta versión, no sobre {p:?}"
            ),
            MotivoAutoria::MetaFueraDeEsquema(v) => write!(
                f,
                "la metavariable {v:?} no está ligada por ningún `esquema`"
            ),
            MotivoAutoria::MetaSinUso(v) => write!(
                f,
                "el `esquema` liga la metavariable {v:?} y no la usa en ninguna posición de constante"
            ),
            MotivoAutoria::DominioDeEsquemaDesconocido(d) => write!(
                f,
                "el dominio de esquema {d:?} no es Lab, KV, KE ni Dom.<Predicado>"
            ),
            MotivoAutoria::ConstanteMalFormada { texto, motivo } => {
                write!(f, "la constante {texto:?} no es válida: {motivo}")
            }
            MotivoAutoria::AnidamientoExcesivo { limite } => write!(
                f,
                "la S-expresión anida más de {limite} niveles; el parser no baja más"
            ),
            MotivoAutoria::ColisionConMetavariable(v) => {
                write!(f, "{v:?} ya es metavariable del `esquema` envolvente; ")?;
                write!(
                    f,
                    "un cuantificador no puede ligar ese nombre, porque el mismo "
                )?;
                write!(
                    f,
                    "identificador denotaría dos cosas distintas según dónde aparezca"
                )
            }
            MotivoAutoria::NombreReservado(v) => write!(
                f,
                "el nombre {v:?} usa el prefijo `%`, reservado por el motor"
            ),
            MotivoAutoria::TextoVacio => write!(f, "el texto no contiene ninguna S-expresión"),
            MotivoAutoria::TextoSobrante => {
                write!(f, "hay texto después de la S-expresión de nivel superior")
            }
        }
    }
}

impl std::fmt::Display for ErrorAutoria {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.linea, self.columna, self.motivo)
    }
}

impl std::error::Error for ErrorAutoria {}

/// Un token con su posición en el texto, en base 1.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Tok {
    clase: Clase,
    linea: usize,
    columna: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Clase {
    Abre,
    Cierra,
    /// Símbolo: corrida de caracteres que no es blanco, paréntesis ni
    /// comilla.
    Simbolo(String),
    /// Cadena entre comillas dobles, ya desescapada.
    Cadena(String),
}

/// Árbol de S-expresiones sin interpretar: la etapa intermedia entre el
/// léxico y el AST, para que los errores puedan citar la posición del token
/// exacto que los provocó.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Sexp {
    Hoja(Tok),
    Lista {
        elementos: Vec<Sexp>,
        linea: usize,
        columna: usize,
    },
}

impl Sexp {
    fn posicion(&self) -> (usize, usize) {
        match self {
            Sexp::Hoja(t) => (t.linea, t.columna),
            Sexp::Lista { linea, columna, .. } => (*linea, *columna),
        }
    }

    fn error(&self, motivo: MotivoAutoria) -> ErrorAutoria {
        let (linea, columna) = self.posicion();
        ErrorAutoria {
            linea,
            columna,
            motivo,
        }
    }

    /// El símbolo de esta hoja, si es una hoja de símbolo.
    fn simbolo(&self) -> Option<&str> {
        match self {
            Sexp::Hoja(Tok {
                clase: Clase::Simbolo(s),
                ..
            }) => Some(s),
            _ => None,
        }
    }
}

/// Divide el texto en tokens, descartando blancos y comentarios (`;` hasta
/// fin de línea, como SMT-LIB).
fn lexar(texto: &str) -> Result<Vec<Tok>, ErrorAutoria> {
    let mut tokens = Vec::new();
    let mut linea = 1usize;
    let mut columna = 1usize;
    let mut chars = texto.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            '\n' => {
                chars.next();
                linea += 1;
                columna = 1;
            }
            c if c.is_whitespace() => {
                chars.next();
                columna += 1;
            }
            ';' => {
                while let Some(&c) = chars.peek() {
                    if c == '\n' {
                        break;
                    }
                    chars.next();
                    columna += 1;
                }
            }
            '(' | ')' => {
                chars.next();
                let clase = if c == '(' { Clase::Abre } else { Clase::Cierra };
                tokens.push(Tok {
                    clase,
                    linea,
                    columna,
                });
                columna += 1;
            }
            '"' => {
                let (linea_inicio, columna_inicio) = (linea, columna);
                chars.next();
                columna += 1;
                let mut texto_cadena = String::new();
                let mut cerrada = false;
                while let Some(c) = chars.next() {
                    columna += 1;
                    match c {
                        '"' => {
                            cerrada = true;
                            break;
                        }
                        '\n' => {
                            linea += 1;
                            columna = 1;
                            texto_cadena.push('\n');
                        }
                        '\\' => {
                            let siguiente = chars.next();
                            columna += 1;
                            match siguiente {
                                Some('\\') => texto_cadena.push('\\'),
                                Some('"') => texto_cadena.push('"'),
                                _ => {
                                    return Err(ErrorAutoria {
                                        linea,
                                        columna,
                                        motivo: MotivoAutoria::EscapeInvalido,
                                    });
                                }
                            }
                        }
                        otro => texto_cadena.push(otro),
                    }
                }
                if !cerrada {
                    return Err(ErrorAutoria {
                        linea: linea_inicio,
                        columna: columna_inicio,
                        motivo: MotivoAutoria::CadenaSinCerrar,
                    });
                }
                tokens.push(Tok {
                    clase: Clase::Cadena(texto_cadena),
                    linea: linea_inicio,
                    columna: columna_inicio,
                });
            }
            _ => {
                let (linea_inicio, columna_inicio) = (linea, columna);
                let mut simbolo = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() || c == '(' || c == ')' || c == '"' || c == ';' {
                        break;
                    }
                    simbolo.push(c);
                    chars.next();
                    columna += 1;
                }
                tokens.push(Tok {
                    clase: Clase::Simbolo(simbolo),
                    linea: linea_inicio,
                    columna: columna_inicio,
                });
            }
        }
    }
    Ok(tokens)
}

/// Arma el árbol de S-expresiones a partir de los tokens, exigiendo
/// exactamente una expresión de nivel superior.
fn arbol(tokens: &[Tok]) -> Result<Sexp, ErrorAutoria> {
    let mut posicion = 0usize;
    let Some(primero) = tokens.first() else {
        return Err(ErrorAutoria {
            linea: 1,
            columna: 1,
            motivo: MotivoAutoria::TextoVacio,
        });
    };
    if primero.clase == Clase::Cierra {
        return Err(ErrorAutoria {
            linea: primero.linea,
            columna: primero.columna,
            motivo: MotivoAutoria::ParentesisSobrante,
        });
    }
    let raiz = arbol_desde(tokens, &mut posicion, 1)?;
    if posicion != tokens.len() {
        let sobrante = &tokens[posicion];
        let motivo = if sobrante.clase == Clase::Cierra {
            MotivoAutoria::ParentesisSobrante
        } else {
            MotivoAutoria::TextoSobrante
        };
        return Err(ErrorAutoria {
            linea: sobrante.linea,
            columna: sobrante.columna,
            motivo,
        });
    }
    Ok(raiz)
}

/// `profundidad` es el nivel de la lista que se está armando, contando
/// desde 1 para la de nivel superior. La recursión consume una trama de
/// pila nativa por nivel, y Rust no deja comprobar el espacio de pila
/// disponible: la cota es lo que convierte una S-expresión patológica en un
/// `ErrorAutoria` en vez de en un aborto del proceso.
fn arbol_desde(
    tokens: &[Tok],
    posicion: &mut usize,
    profundidad: usize,
) -> Result<Sexp, ErrorAutoria> {
    let tok = tokens[*posicion].clone();
    *posicion += 1;
    match tok.clase {
        Clase::Abre => {
            if profundidad > PROFUNDIDAD_MAXIMA {
                return Err(ErrorAutoria {
                    linea: tok.linea,
                    columna: tok.columna,
                    motivo: MotivoAutoria::AnidamientoExcesivo {
                        limite: PROFUNDIDAD_MAXIMA,
                    },
                });
            }
            let mut elementos = Vec::new();
            loop {
                let Some(siguiente) = tokens.get(*posicion) else {
                    return Err(ErrorAutoria {
                        linea: tok.linea,
                        columna: tok.columna,
                        motivo: MotivoAutoria::ParentesisSinCerrar,
                    });
                };
                if siguiente.clase == Clase::Cierra {
                    *posicion += 1;
                    return Ok(Sexp::Lista {
                        elementos,
                        linea: tok.linea,
                        columna: tok.columna,
                    });
                }
                elementos.push(arbol_desde(tokens, posicion, profundidad + 1)?);
            }
        }
        Clase::Cierra => Err(ErrorAutoria {
            linea: tok.linea,
            columna: tok.columna,
            motivo: MotivoAutoria::ParentesisSobrante,
        }),
        _ => Ok(Sexp::Hoja(tok)),
    }
}

/// Contexto léxico del parseo: las variables ligadas por los
/// cuantificadores envolventes (una pila: el último elemento es el más
/// interno), las metavariables del `esquema` de nivel superior y cuáles de
/// ellas se usaron, más el contador de nombres frescos que genera el motor
/// al expandir azúcar.
#[derive(Default)]
struct Contexto {
    variables: Vec<(String, SorteVar)>,
    /// Toda ligadura vista en el invariante, sin truncar al cerrar un
    /// alcance: la unicidad de sorte es una regla del invariante entero, no
    /// del alcance léxico (spec §3.1), así que dos cuantificadores hermanos
    /// tampoco pueden darle sortes distintas al mismo nombre.
    declaradas: Vec<(String, SorteVar)>,
    /// Nivel de anidamiento de la fórmula que se está traduciendo.
    profundidad: usize,
    metas: Vec<String>,
    metas_usadas: BTreeSet<String>,
    frescas: usize,
}

impl Contexto {
    fn buscar(&self, nombre: &str) -> Option<SorteVar> {
        self.variables
            .iter()
            .rev()
            .find(|(n, _)| n == nombre)
            .map(|(_, s)| *s)
    }

    fn es_meta(&self, nombre: &str) -> bool {
        self.metas.iter().any(|m| m == nombre)
    }

    /// Nombre fresco para una ligadura que genera el motor, no el autor: el
    /// prefijo `%` está reservado, así que no puede chocar con nada escrito.
    fn fresca(&mut self) -> Var {
        let nombre = format!("{PREFIJO_GENERADO}p{}", self.frescas);
        self.frescas += 1;
        Var::nuevo(&nombre)
    }
}

/// Profundidad máxima de anidamiento que el parser acepta. Tanto el armado
/// del árbol como la traducción al AST son recursivos, y la pila nativa no
/// es un recurso que Rust deje comprobar: sin esta cota, una S-expresión lo
/// bastante anidada aborta el proceso en vez de devolver un `ErrorAutoria`.
///
/// El valor sale de una medición, no de una corazonada. Sobre un hilo con
/// pila de 512 KiB y binario de depuración —el caso más exigente, porque
/// ahí cada nivel ocupa varios KiB de trama— el parser resuelve 48 niveles
/// y se cae en 64. La cota queda en 32: por debajo de lo medido en el
/// entorno más apretado, y muy por encima de cualquier invariante escrito a
/// mano (los del cap. 07 no pasan de cinco niveles). `la_cota_de_anidamiento_aguanta_en_un_hilo_de_pila_reducida`
/// mide esto en cada corrida en vez de dejarlo enunciado.
pub(crate) const PROFUNDIDAD_MAXIMA: usize = 32;

/// Marca obligatoria de una constante de nodo o de arista (spec §4.2): sin
/// ella, un nombre de variable mal tecleado se volvería en silencio una
/// constante inexistente.
const MARCA_CONSTANTE: char = '@';

/// Palabra clave que introduce la guarda explícita de un cuantificador.
const CLAVE_GUARDA: &str = ":guarda";

/// Sorte de un símbolo en posición de nodo o de arista: la de su ligadura,
/// o la que se deduce de la forma de la constante marcada (una constante de
/// arista lleva la flecha de su forma canónica).
fn sorte_de_simbolo(sexp: &Sexp, contexto: &Contexto) -> Result<SorteVar, ErrorAutoria> {
    let simbolo = sexp
        .simbolo()
        .ok_or_else(|| sexp.error(MotivoAutoria::TokenInesperado("símbolo".to_string())))?;
    if let Some(resto) = simbolo.strip_prefix(MARCA_CONSTANTE) {
        return Ok(if resto.contains('→') {
            SorteVar::Arista
        } else {
            SorteVar::Nodo
        });
    }
    contexto
        .buscar(simbolo)
        .ok_or_else(|| sexp.error(MotivoAutoria::SimboloLibre(simbolo.to_string())))
}

fn termino_nodo(
    sexp: &Sexp,
    contexto: &Contexto,
    forma: &str,
    posicion: usize,
) -> Result<TerminoNodo, ErrorAutoria> {
    let simbolo = sexp
        .simbolo()
        .ok_or_else(|| sexp.error(MotivoAutoria::TokenInesperado("símbolo".to_string())))?;
    if let Some(ruta) = simbolo.strip_prefix(MARCA_CONSTANTE) {
        if ruta.contains('→') {
            return Err(sexp.error(MotivoAutoria::SorteEquivocadaEnPosicion {
                forma: forma.to_string(),
                posicion,
            }));
        }
        let path = BundlePath::nuevo(ruta).map_err(|error| {
            sexp.error(MotivoAutoria::ConstanteMalFormada {
                texto: simbolo.to_string(),
                motivo: error.to_string(),
            })
        })?;
        return Ok(TerminoNodo::Constante(path));
    }
    match contexto.buscar(simbolo) {
        Some(SorteVar::Nodo) => Ok(TerminoNodo::Var(Var::nuevo(simbolo))),
        Some(SorteVar::Arista) => Err(sexp.error(MotivoAutoria::SorteEquivocadaEnPosicion {
            forma: forma.to_string(),
            posicion,
        })),
        None => Err(sexp.error(MotivoAutoria::SimboloLibre(simbolo.to_string()))),
    }
}

fn termino_arista(
    sexp: &Sexp,
    contexto: &Contexto,
    forma: &str,
    posicion: usize,
) -> Result<TerminoArista, ErrorAutoria> {
    let simbolo = sexp
        .simbolo()
        .ok_or_else(|| sexp.error(MotivoAutoria::TokenInesperado("símbolo".to_string())))?;
    if let Some(texto) = simbolo.strip_prefix(MARCA_CONSTANTE) {
        let id = IdArista::parsear(texto).map_err(|error| {
            sexp.error(MotivoAutoria::ConstanteMalFormada {
                texto: simbolo.to_string(),
                motivo: error.to_string(),
            })
        })?;
        return Ok(TerminoArista::Constante(id));
    }
    match contexto.buscar(simbolo) {
        Some(SorteVar::Arista) => Ok(TerminoArista::Var(Var::nuevo(simbolo))),
        Some(SorteVar::Nodo) => Err(sexp.error(MotivoAutoria::SorteEquivocadaEnPosicion {
            forma: forma.to_string(),
            posicion,
        })),
        None => Err(sexp.error(MotivoAutoria::SimboloLibre(simbolo.to_string()))),
    }
}

/// Símbolo literal en una posición que no es de sorte N ni E (tipo,
/// constante de extensión): o es una metavariable del esquema envolvente, o
/// es el literal escrito tal cual.
fn simbolo_literal_o_meta<'a>(
    sexp: &'a Sexp,
    contexto: &mut Contexto,
) -> Result<(&'a str, bool), ErrorAutoria> {
    let simbolo = sexp
        .simbolo()
        .ok_or_else(|| sexp.error(MotivoAutoria::TokenInesperado("símbolo".to_string())))?;
    if simbolo.starts_with(MARCA_CONSTANTE) {
        return Err(sexp.error(MotivoAutoria::TokenInesperado(simbolo.to_string())));
    }
    if contexto.es_meta(simbolo) {
        contexto.metas_usadas.insert(simbolo.to_string());
        return Ok((simbolo, true));
    }
    if contexto.buscar(simbolo).is_some() {
        return Err(sexp.error(MotivoAutoria::TokenInesperado(simbolo.to_string())));
    }
    Ok((simbolo, false))
}

fn termino_sorte(sexp: &Sexp, contexto: &mut Contexto) -> Result<TerminoSorte, ErrorAutoria> {
    let (simbolo, es_meta) = simbolo_literal_o_meta(sexp, contexto)?;
    Ok(if es_meta {
        TerminoSorte::Meta(Var::nuevo(simbolo))
    } else {
        TerminoSorte::Literal(Sorte::nuevo(simbolo))
    })
}

fn termino_const_ext(
    sexp: &Sexp,
    contexto: &mut Contexto,
) -> Result<TerminoConstExt, ErrorAutoria> {
    let (simbolo, es_meta) = simbolo_literal_o_meta(sexp, contexto)?;
    Ok(if es_meta {
        TerminoConstExt::Meta(Var::nuevo(simbolo))
    } else {
        TerminoConstExt::Literal(simbolo.to_string())
    })
}

/// Etiqueta: cadena entre comillas, o metavariable de un esquema. Un
/// símbolo suelto ahí es un error de autoría — la etiqueta se escribe entre
/// comillas.
fn termino_etiqueta(sexp: &Sexp, contexto: &mut Contexto) -> Result<TerminoEtiqueta, ErrorAutoria> {
    if let Sexp::Hoja(Tok {
        clase: Clase::Cadena(texto),
        ..
    }) = sexp
    {
        return Ok(TerminoEtiqueta::Literal(Etiqueta::nuevo(texto)));
    }
    let simbolo = sexp
        .simbolo()
        .ok_or_else(|| sexp.error(MotivoAutoria::TokenInesperado("etiqueta".to_string())))?;
    if contexto.es_meta(simbolo) {
        contexto.metas_usadas.insert(simbolo.to_string());
        return Ok(TerminoEtiqueta::Meta(Var::nuevo(simbolo)));
    }
    Err(sexp.error(MotivoAutoria::TokenInesperado(simbolo.to_string())))
}

fn exigir_aridad(
    sexp: &Sexp,
    forma: &str,
    exacta: usize,
    encontrada: usize,
) -> Result<(), ErrorAutoria> {
    if encontrada != exacta {
        return Err(sexp.error(MotivoAutoria::AridadIncorrecta {
            forma: forma.to_string(),
            esperada: exacta.to_string(),
            encontrada,
        }));
    }
    Ok(())
}

fn exigir_al_menos(
    sexp: &Sexp,
    forma: &str,
    minima: usize,
    encontrada: usize,
) -> Result<(), ErrorAutoria> {
    if encontrada < minima {
        return Err(sexp.error(MotivoAutoria::AridadIncorrecta {
            forma: forma.to_string(),
            esperada: format!("al menos {minima}"),
            encontrada,
        }));
    }
    Ok(())
}

fn ligaduras(sexp: &Sexp) -> Result<Vec<(Var, SorteVar)>, ErrorAutoria> {
    let Sexp::Lista { elementos, .. } = sexp else {
        return Err(sexp.error(MotivoAutoria::TokenInesperado(
            "lista de ligaduras".to_string(),
        )));
    };
    let mut resultado: Vec<(Var, SorteVar)> = Vec::new();
    for par in elementos {
        let Sexp::Lista {
            elementos: campos, ..
        } = par
        else {
            return Err(par.error(MotivoAutoria::TokenInesperado("(nombre sorte)".to_string())));
        };
        exigir_aridad(par, "ligadura", 2, campos.len())?;
        let nombre = nombre_de_ligadura(&campos[0])?;
        let sorte = match campos[1].simbolo() {
            Some("N") => SorteVar::Nodo,
            Some("E") => SorteVar::Arista,
            Some(otro) => {
                return Err(
                    campos[1].error(MotivoAutoria::SorteDeVariableDesconocida(otro.to_string()))
                );
            }
            None => {
                return Err(campos[1].error(MotivoAutoria::TokenInesperado("sorte".to_string())));
            }
        };
        if resultado.iter().any(|(v, _)| v.como_str() == nombre) {
            return Err(campos[0].error(MotivoAutoria::VariableRedeclarada(nombre.to_string())));
        }
        resultado.push((Var::nuevo(nombre), sorte));
    }
    Ok(resultado)
}

fn nombre_de_ligadura(sexp: &Sexp) -> Result<&str, ErrorAutoria> {
    let nombre = sexp
        .simbolo()
        .ok_or_else(|| sexp.error(MotivoAutoria::TokenInesperado("nombre".to_string())))?;
    if nombre.starts_with(PREFIJO_GENERADO) {
        return Err(sexp.error(MotivoAutoria::NombreReservado(nombre.to_string())));
    }
    if nombre.starts_with(MARCA_CONSTANTE) {
        return Err(sexp.error(MotivoAutoria::TokenInesperado(nombre.to_string())));
    }
    Ok(nombre)
}

fn cuantificador(
    sexp: &Sexp,
    cabeza: &str,
    argumentos: &[Sexp],
    contexto: &mut Contexto,
) -> Result<Formula, ErrorAutoria> {
    let con_guarda = argumentos
        .get(1)
        .and_then(Sexp::simbolo)
        .is_some_and(|s| s == CLAVE_GUARDA);
    let esperada = if con_guarda { 4 } else { 2 };
    exigir_aridad(sexp, cabeza, esperada, argumentos.len())?;
    let nuevas = ligaduras(&argumentos[0])?;
    // Cada ligadura se compara contra su propio par `(nombre sorte)` del
    // texto, no contra la lista entera: el error debe señalar el binder que
    // colisiona y no el paréntesis que abre a todos.
    let pares: &[Sexp] = match &argumentos[0] {
        Sexp::Lista { elementos, .. } => elementos,
        hoja => std::slice::from_ref(hoja),
    };
    for (indice, (nombre, sorte)) in nuevas.iter().enumerate() {
        let donde = pares.get(indice).unwrap_or(&argumentos[0]);
        // Una metavariable de esquema y una variable ligada viven en
        // posiciones sintácticas distintas —constante frente a nodo o
        // arista—, así que sin esta comprobación el mismo identificador
        // podría denotar las dos cosas dentro del mismo AST y cuál gana
        // dependería de dónde aparece.
        if contexto.es_meta(nombre.como_str()) {
            return Err(donde.error(MotivoAutoria::ColisionConMetavariable(
                nombre.como_str().to_string(),
            )));
        }
        let religa_en_alcance = contexto.buscar(nombre.como_str()).is_some();
        let cambia_de_sorte = contexto
            .declaradas
            .iter()
            .any(|(n, s)| n == nombre.como_str() && s != sorte);
        if religa_en_alcance || cambia_de_sorte {
            return Err(donde.error(MotivoAutoria::VariableRedeclarada(
                nombre.como_str().to_string(),
            )));
        }
    }
    for (nombre, sorte) in &nuevas {
        contexto
            .declaradas
            .push((nombre.como_str().to_string(), *sorte));
    }
    let cuantas = nuevas.len();
    for (nombre, sorte) in &nuevas {
        contexto
            .variables
            .push((nombre.como_str().to_string(), *sorte));
    }
    let interior = (|| {
        let guarda = if con_guarda {
            Some(Box::new(formula(&argumentos[2], contexto)?))
        } else {
            None
        };
        let cuerpo = Box::new(formula(
            argumentos.last().expect("aridad verificada"),
            contexto,
        )?);
        Ok((guarda, cuerpo))
    })();
    contexto
        .variables
        .truncate(contexto.variables.len() - cuantas);
    let (guarda, cuerpo) = interior?;
    Ok(if cabeza == "forall" {
        Formula::ParaTodo {
            ligaduras: nuevas,
            guarda,
            cuerpo,
        }
    } else {
        Formula::Existe {
            ligaduras: nuevas,
            guarda,
            cuerpo,
        }
    })
}

/// `(Raiz x)` ≡ `(not (exists ((%pN N)) (Parent x %pN)))`: el azúcar del
/// capítulo 01, cuyo cuantificador interno no cuenta para la condición de
/// relativización guardada.
fn raiz(
    sexp: &Sexp,
    argumentos: &[Sexp],
    contexto: &mut Contexto,
) -> Result<Formula, ErrorAutoria> {
    exigir_aridad(sexp, "Raiz", 1, argumentos.len())?;
    let hijo = termino_nodo(&argumentos[0], contexto, "Raiz", 1)?;
    let padre = contexto.fresca();
    Ok(Formula::No(Box::new(Formula::Existe {
        ligaduras: vec![(padre.clone(), SorteVar::Nodo)],
        guarda: None,
        cuerpo: Box::new(Formula::Parent {
            hijo,
            padre: TerminoNodo::Var(padre),
        }),
    })))
}

fn igualdad(
    sexp: &Sexp,
    argumentos: &[Sexp],
    contexto: &Contexto,
) -> Result<Formula, ErrorAutoria> {
    exigir_aridad(sexp, "=", 2, argumentos.len())?;
    let izquierda = sorte_de_simbolo(&argumentos[0], contexto)?;
    let derecha = sorte_de_simbolo(&argumentos[1], contexto)?;
    if izquierda != derecha {
        return Err(
            argumentos[1].error(MotivoAutoria::SorteEquivocadaEnPosicion {
                forma: "=".to_string(),
                posicion: 2,
            }),
        );
    }
    Ok(match izquierda {
        SorteVar::Nodo => Formula::IgualNodo(
            termino_nodo(&argumentos[0], contexto, "=", 1)?,
            termino_nodo(&argumentos[1], contexto, "=", 2)?,
        ),
        SorteVar::Arista => Formula::IgualArista(
            termino_arista(&argumentos[0], contexto, "=", 1)?,
            termino_arista(&argumentos[1], contexto, "=", 2)?,
        ),
    })
}

fn haslab(
    sexp: &Sexp,
    argumentos: &[Sexp],
    contexto: &mut Contexto,
) -> Result<Formula, ErrorAutoria> {
    exigir_aridad(sexp, "HasLab", 2, argumentos.len())?;
    let sujeto = match sorte_de_simbolo(&argumentos[0], contexto)? {
        SorteVar::Nodo => SujetoTermino::Nodo(termino_nodo(&argumentos[0], contexto, "HasLab", 1)?),
        SorteVar::Arista => {
            SujetoTermino::Arista(termino_arista(&argumentos[0], contexto, "HasLab", 1)?)
        }
    };
    Ok(Formula::HasLab {
        sujeto,
        etiqueta: termino_etiqueta(&argumentos[1], contexto)?,
    })
}

fn conjuncion_o_disyuncion(
    sexp: &Sexp,
    cabeza: &str,
    argumentos: &[Sexp],
    contexto: &mut Contexto,
) -> Result<Formula, ErrorAutoria> {
    exigir_al_menos(sexp, cabeza, 1, argumentos.len())?;
    let mut partes = Vec::with_capacity(argumentos.len());
    for argumento in argumentos {
        partes.push(formula(argumento, contexto)?);
    }
    Ok(if cabeza == "and" {
        Formula::Y(partes)
    } else {
        Formula::O(partes)
    })
}

/// Traduce una S-expresión al AST, llevando la cuenta del anidamiento: la
/// recursión mutua entre esta función, `cuantificador` y
/// `conjuncion_o_disyuncion` consume pila nativa por nivel igual que el
/// armado del árbol, así que lleva la misma cota.
fn formula(sexp: &Sexp, contexto: &mut Contexto) -> Result<Formula, ErrorAutoria> {
    if contexto.profundidad >= PROFUNDIDAD_MAXIMA {
        return Err(sexp.error(MotivoAutoria::AnidamientoExcesivo {
            limite: PROFUNDIDAD_MAXIMA,
        }));
    }
    contexto.profundidad += 1;
    let resultado = formula_interna(sexp, contexto);
    contexto.profundidad -= 1;
    resultado
}

fn formula_interna(sexp: &Sexp, contexto: &mut Contexto) -> Result<Formula, ErrorAutoria> {
    let Sexp::Lista { elementos, .. } = sexp else {
        let texto = sexp.simbolo().unwrap_or("cadena").to_string();
        return Err(sexp.error(MotivoAutoria::FormaDesconocida(texto)));
    };
    let Some(cabeza) = elementos.first().and_then(Sexp::simbolo) else {
        return Err(sexp.error(MotivoAutoria::FormaDesconocida("()".to_string())));
    };
    let argumentos = &elementos[1..];
    match cabeza {
        "forall" | "exists" => cuantificador(sexp, cabeza, argumentos, contexto),
        "and" | "or" => conjuncion_o_disyuncion(sexp, cabeza, argumentos, contexto),
        "not" => {
            exigir_aridad(sexp, "not", 1, argumentos.len())?;
            Ok(Formula::No(Box::new(formula(&argumentos[0], contexto)?)))
        }
        "=>" => {
            exigir_aridad(sexp, "=>", 2, argumentos.len())?;
            Ok(Formula::Implica(
                Box::new(formula(&argumentos[0], contexto)?),
                Box::new(formula(&argumentos[1], contexto)?),
            ))
        }
        "=" => igualdad(sexp, argumentos, contexto),
        "Type" => {
            exigir_aridad(sexp, "Type", 2, argumentos.len())?;
            Ok(Formula::Type {
                sujeto: termino_nodo(&argumentos[0], contexto, "Type", 1)?,
                sorte: termino_sorte(&argumentos[1], contexto)?,
            })
        }
        "Edge" => {
            exigir_aridad(sexp, "Edge", 4, argumentos.len())?;
            Ok(Formula::Edge {
                arista: termino_arista(&argumentos[0], contexto, "Edge", 1)?,
                origen: termino_nodo(&argumentos[1], contexto, "Edge", 2)?,
                destino: termino_nodo(&argumentos[2], contexto, "Edge", 3)?,
                sorte: termino_sorte(&argumentos[3], contexto)?,
            })
        }
        "Parent" => {
            exigir_aridad(sexp, "Parent", 2, argumentos.len())?;
            Ok(Formula::Parent {
                hijo: termino_nodo(&argumentos[0], contexto, "Parent", 1)?,
                padre: termino_nodo(&argumentos[1], contexto, "Parent", 2)?,
            })
        }
        "HasLab" => haslab(sexp, argumentos, contexto),
        "Raiz" => raiz(sexp, argumentos, contexto),
        "tc" => {
            exigir_aridad(sexp, "tc", 3, argumentos.len())?;
            let relacion = argumentos[0].simbolo().unwrap_or_default();
            if relacion != "Parent" {
                return Err(
                    argumentos[0].error(MotivoAutoria::TcSoloSobreParent(relacion.to_string()))
                );
            }
            Ok(Formula::TcParent {
                desde: termino_nodo(&argumentos[1], contexto, "tc", 2)?,
                hasta: termino_nodo(&argumentos[2], contexto, "tc", 3)?,
            })
        }
        otro => {
            // Todo símbolo de cabeza que no sea una forma reservada es un
            // predicado de extensión, y su aridad es 2: decir «forma
            // desconocida» mandaría al autor a revisar el nombre cuando lo
            // que está mal es la cantidad de argumentos.
            exigir_aridad(sexp, otro, 2, argumentos.len())?;
            Ok(Formula::Ext {
                predicado: otro.to_string(),
                sujeto: termino_nodo(&argumentos[0], contexto, otro, 1)?,
                constante: termino_const_ext(&argumentos[1], contexto)?,
            })
        }
    }
}

/// Ligaduras de un `esquema`: `(c Lab)`, `(k KV)`, `(k KE)`, `(c Dom.P)`.
fn ligaduras_esquema(sexp: &Sexp) -> Result<Vec<(Var, DominioEsquema)>, ErrorAutoria> {
    let Sexp::Lista { elementos, .. } = sexp else {
        return Err(sexp.error(MotivoAutoria::TokenInesperado(
            "lista de ligaduras".to_string(),
        )));
    };
    let mut resultado: Vec<(Var, DominioEsquema)> = Vec::new();
    for par in elementos {
        let Sexp::Lista {
            elementos: campos, ..
        } = par
        else {
            return Err(par.error(MotivoAutoria::TokenInesperado(
                "(nombre dominio)".to_string(),
            )));
        };
        exigir_aridad(par, "ligadura", 2, campos.len())?;
        let nombre = nombre_de_ligadura(&campos[0])?;
        let dominio = match campos[1].simbolo() {
            Some("Lab") => DominioEsquema::Etiquetas,
            Some("KV") => DominioEsquema::SortesNodo,
            Some("KE") => DominioEsquema::SortesArista,
            Some(otro) => match otro.strip_prefix("Dom.") {
                Some(predicado) if !predicado.is_empty() => {
                    DominioEsquema::DominioPredicado(predicado.to_string())
                }
                _ => {
                    return Err(campos[1]
                        .error(MotivoAutoria::DominioDeEsquemaDesconocido(otro.to_string())));
                }
            },
            None => {
                return Err(campos[1].error(MotivoAutoria::TokenInesperado("dominio".to_string())));
            }
        };
        if resultado.iter().any(|(v, _)| v.como_str() == nombre) {
            return Err(campos[0].error(MotivoAutoria::VariableRedeclarada(nombre.to_string())));
        }
        resultado.push((Var::nuevo(nombre), dominio));
    }
    Ok(resultado)
}

/// Parsea la sintaxis de autoría a una [`Sentencia`].
pub fn parsear(texto: &str) -> Result<Sentencia, ErrorAutoria> {
    let tokens = lexar(texto)?;
    let raiz = arbol(&tokens)?;
    let mut contexto = Contexto::default();
    let es_esquema = matches!(&raiz, Sexp::Lista { elementos, .. }
        if elementos.first().and_then(Sexp::simbolo) == Some("esquema"));
    if !es_esquema {
        return Ok(Sentencia::Oracion(formula(&raiz, &mut contexto)?));
    }
    let Sexp::Lista { elementos, .. } = &raiz else {
        unreachable!("ya se comprobó que es una lista");
    };
    let argumentos = &elementos[1..];
    exigir_aridad(&raiz, "esquema", 2, argumentos.len())?;
    let ligaduras = ligaduras_esquema(&argumentos[0])?;
    contexto.metas = ligaduras
        .iter()
        .map(|(v, _)| v.como_str().to_string())
        .collect();
    let cuerpo = formula(&argumentos[1], &mut contexto)?;
    for (variable, _) in &ligaduras {
        if !contexto.metas_usadas.contains(variable.como_str()) {
            return Err(
                argumentos[0].error(MotivoAutoria::MetaSinUso(variable.como_str().to_string()))
            );
        }
    }
    Ok(Sentencia::Esquema { ligaduras, cuerpo })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atomos::IdArista;
    use crate::formula::{
        DominioEsquema, Formula, SorteVar, SujetoTermino, TerminoArista, TerminoConstExt,
        TerminoEtiqueta, TerminoNodo, TerminoSorte, Var,
    };
    use crate::path::BundlePath;
    use crate::path::ErrorPath;
    use crate::tipos::{Etiqueta, Sorte};

    #[test]
    fn parsea_un_parent_con_variable_ligada_y_constante_marcada() {
        let s = parsear("(forall ((x N)) (Parent x @rules/index))").unwrap();
        assert_eq!(
            s,
            Sentencia::Oracion(Formula::ParaTodo {
                ligaduras: vec![(Var::nuevo("x"), SorteVar::Nodo)],
                guarda: None,
                cuerpo: Box::new(Formula::Parent {
                    hijo: TerminoNodo::Var(Var::nuevo("x")),
                    padre: TerminoNodo::Constante(BundlePath::nuevo("rules/index").unwrap()),
                }),
            })
        );
    }

    fn oracion(texto: &str) -> Formula {
        match parsear(texto).unwrap_or_else(|e| panic!("{texto}: {e}")) {
            Sentencia::Oracion(f) => f,
            otra => panic!("se esperaba una oración y salió {otra:?}"),
        }
    }

    fn motivo(texto: &str) -> MotivoAutoria {
        match parsear(texto) {
            Ok(s) => panic!("debería rechazar {texto:?} y devolvió {s:?}"),
            Err(e) => e.motivo,
        }
    }

    #[test]
    fn parsea_el_invariante_i1_del_capitulo_01() {
        let f = oracion(
            "(forall ((x N) (y N))
               (not (exists ((e E))
                      (and (Effect x Puro) (Effect y Adaptador) (Edge e x y delta)))))",
        );
        let Formula::ParaTodo {
            ligaduras, cuerpo, ..
        } = f
        else {
            panic!("se esperaba un ∀");
        };
        assert_eq!(ligaduras.len(), 2);
        let Formula::No(interior) = *cuerpo else {
            panic!("se esperaba una negación");
        };
        let Formula::Existe { cuerpo, .. } = *interior else {
            panic!("se esperaba un ∃");
        };
        let Formula::Y(partes) = *cuerpo else {
            panic!("se esperaba una conjunción");
        };
        assert_eq!(
            partes[0],
            Formula::Ext {
                predicado: "Effect".to_string(),
                sujeto: TerminoNodo::Var(Var::nuevo("x")),
                constante: TerminoConstExt::Literal("Puro".to_string()),
            }
        );
        assert_eq!(
            partes[2],
            Formula::Edge {
                arista: TerminoArista::Var(Var::nuevo("e")),
                origen: TerminoNodo::Var(Var::nuevo("x")),
                destino: TerminoNodo::Var(Var::nuevo("y")),
                sorte: TerminoSorte::Literal(Sorte::nuevo("delta")),
            }
        );
    }

    #[test]
    fn parsea_type_haslab_igualdad_disyuncion_e_implicacion() {
        let f = oracion(
            "(forall ((x N) (e E))
               (=> (or (Type x rule) (HasLab x \"un \\\"título\\\"\"))
                   (and (= x @rules/a) (not (= e @rules/a→rules/b#rel#0)))))",
        );
        let Formula::ParaTodo { cuerpo, .. } = f else {
            panic!("se esperaba un ∀");
        };
        let Formula::Implica(antecedente, consecuente) = *cuerpo else {
            panic!("se esperaba una implicación");
        };
        let Formula::O(alternativas) = *antecedente else {
            panic!("se esperaba una disyunción");
        };
        assert_eq!(
            alternativas[0],
            Formula::Type {
                sujeto: TerminoNodo::Var(Var::nuevo("x")),
                sorte: TerminoSorte::Literal(Sorte::nuevo("rule")),
            }
        );
        assert_eq!(
            alternativas[1],
            Formula::HasLab {
                sujeto: SujetoTermino::Nodo(TerminoNodo::Var(Var::nuevo("x"))),
                etiqueta: TerminoEtiqueta::Literal(Etiqueta::nuevo("un \"título\"")),
            }
        );
        let Formula::Y(partes) = *consecuente else {
            panic!("se esperaba una conjunción");
        };
        assert!(matches!(partes[0], Formula::IgualNodo(..)));
        assert!(matches!(partes[1], Formula::No(ref b) if matches!(**b, Formula::IgualArista(..))));
    }

    #[test]
    fn la_guarda_explicita_se_conserva_como_campo_del_ast() {
        let f = oracion("(forall ((x N)) :guarda (Type x rule) (Parent x @rules/index))");
        let Formula::ParaTodo { guarda, .. } = f else {
            panic!("se esperaba un ∀");
        };
        assert_eq!(
            guarda.map(|g| *g),
            Some(Formula::Type {
                sujeto: TerminoNodo::Var(Var::nuevo("x")),
                sorte: TerminoSorte::Literal(Sorte::nuevo("rule")),
            })
        );
    }

    #[test]
    fn la_forma_azucarada_con_implicacion_deja_la_guarda_vacia() {
        let f = oracion("(forall ((x N)) (=> (Type x rule) (Parent x @rules/index)))");
        assert!(matches!(f, Formula::ParaTodo { guarda: None, .. }));
    }

    #[test]
    fn tc_solo_se_aplica_sobre_parent() {
        let f = oracion("(forall ((x N) (y N)) (tc Parent x y))");
        let Formula::ParaTodo { cuerpo, .. } = f else {
            panic!("se esperaba un ∀");
        };
        assert_eq!(
            *cuerpo,
            Formula::TcParent {
                desde: TerminoNodo::Var(Var::nuevo("x")),
                hasta: TerminoNodo::Var(Var::nuevo("y")),
            }
        );
        assert_eq!(
            motivo("(forall ((x N) (y N)) (tc Edge x y))"),
            MotivoAutoria::TcSoloSobreParent("Edge".to_string())
        );
    }

    #[test]
    fn raiz_es_azucar_de_la_negacion_de_un_parent_existencial() {
        let f = oracion("(forall ((x N)) (Raiz x))");
        let Formula::ParaTodo { cuerpo, .. } = f else {
            panic!("se esperaba un ∀");
        };
        let Formula::No(interior) = *cuerpo else {
            panic!("Raiz debe expandirse a una negación");
        };
        let Formula::Existe {
            ligaduras, cuerpo, ..
        } = *interior
        else {
            panic!("Raiz debe expandirse a ¬∃");
        };
        assert_eq!(ligaduras.len(), 1);
        assert!(ligaduras[0].0.como_str().starts_with('%'));
        assert_eq!(
            *cuerpo,
            Formula::Parent {
                hijo: TerminoNodo::Var(Var::nuevo("x")),
                padre: TerminoNodo::Var(ligaduras[0].0.clone()),
            }
        );
    }

    #[test]
    fn un_esquema_liga_su_metavariable_en_posicion_de_constante() {
        let s = parsear(
            "(esquema ((l Lab))
               (forall ((x N) (y N))
                 (=> (and (HasLab x l) (HasLab y l)) (= x y))))",
        )
        .unwrap();
        let Sentencia::Esquema { ligaduras, cuerpo } = s else {
            panic!("se esperaba un esquema");
        };
        assert_eq!(
            ligaduras,
            vec![(Var::nuevo("l"), DominioEsquema::Etiquetas)]
        );
        let Formula::ParaTodo { cuerpo, .. } = cuerpo else {
            panic!("se esperaba un ∀");
        };
        let Formula::Implica(antecedente, _) = *cuerpo else {
            panic!("se esperaba una implicación");
        };
        let Formula::Y(partes) = *antecedente else {
            panic!("se esperaba una conjunción");
        };
        assert_eq!(
            partes[0],
            Formula::HasLab {
                sujeto: SujetoTermino::Nodo(TerminoNodo::Var(Var::nuevo("x"))),
                etiqueta: TerminoEtiqueta::Meta(Var::nuevo("l")),
            }
        );
    }

    #[test]
    fn los_dominios_de_esquema_son_lab_kv_ke_y_dom_de_un_predicado() {
        for (texto, esperado) in [
            ("KV", DominioEsquema::SortesNodo),
            ("KE", DominioEsquema::SortesArista),
            (
                "Dom.Standing",
                DominioEsquema::DominioPredicado("Standing".to_string()),
            ),
        ] {
            let s = parsear(&format!(
                "(esquema ((c {texto})) (forall ((x N)) (Standing x c)))"
            ))
            .unwrap();
            let Sentencia::Esquema { ligaduras, .. } = s else {
                panic!("se esperaba un esquema");
            };
            assert_eq!(ligaduras[0].1, esperado);
        }
        assert_eq!(
            motivo("(esquema ((c Otro)) (forall ((x N)) (Standing x c)))"),
            MotivoAutoria::DominioDeEsquemaDesconocido("Otro".to_string())
        );
    }

    #[test]
    fn un_esquema_que_no_usa_su_metavariable_se_rechaza() {
        assert_eq!(
            motivo("(esquema ((l Lab)) (forall ((x N)) (Type x rule)))"),
            MotivoAutoria::MetaSinUso("l".to_string())
        );
    }

    #[test]
    fn una_etiqueta_solo_se_escribe_entre_comillas_o_como_metavariable() {
        assert_eq!(
            motivo("(forall ((x N)) (HasLab x l))"),
            MotivoAutoria::TokenInesperado("l".to_string())
        );
    }

    #[test]
    fn un_simbolo_libre_sin_marca_de_constante_se_rechaza_con_su_posicion() {
        let error = parsear("(forall ((x N))\n  (Parent x padre))").unwrap_err();
        assert_eq!(
            error.motivo,
            MotivoAutoria::SimboloLibre("padre".to_string())
        );
        assert_eq!((error.linea, error.columna), (2, 13));
    }

    #[test]
    fn una_variable_de_la_otra_sorte_no_sirve_en_posicion_de_nodo() {
        assert!(matches!(
            motivo("(forall ((e E)) (Parent e e))"),
            MotivoAutoria::SorteEquivocadaEnPosicion { .. }
        ));
    }

    #[test]
    fn una_variable_no_se_religa_ni_cambia_de_sorte() {
        assert_eq!(
            motivo("(forall ((x N)) (exists ((x E)) (Edge x @a @b k)))"),
            MotivoAutoria::VariableRedeclarada("x".to_string())
        );
        assert_eq!(
            motivo("(forall ((x N) (x N)) (Parent x x))"),
            MotivoAutoria::VariableRedeclarada("x".to_string())
        );
    }

    #[test]
    fn el_prefijo_de_porcentaje_esta_reservado_para_el_motor() {
        assert_eq!(
            motivo("(forall ((%p N)) (Parent %p %p))"),
            MotivoAutoria::NombreReservado("%p".to_string())
        );
    }

    #[test]
    fn las_aridades_de_las_formas_base_se_exigen() {
        for (texto, forma) in [
            ("(forall ((x N)) (Parent x))", "Parent"),
            ("(forall ((x N) (e E)) (Edge e x x))", "Edge"),
            ("(forall ((x N)) (Type x))", "Type"),
            ("(forall ((x N)) (not))", "not"),
            ("(forall ((x N)) (=> (Type x k)))", "=>"),
        ] {
            assert!(
                matches!(
                    motivo(texto),
                    MotivoAutoria::AridadIncorrecta { forma: ref f, .. } if f == forma
                ),
                "{texto}"
            );
        }
    }

    #[test]
    fn los_desbalances_de_parentesis_y_el_texto_sobrante_se_rechazan() {
        assert_eq!(
            motivo("(forall ((x N)) (Parent x @a)"),
            MotivoAutoria::ParentesisSinCerrar
        );
        assert_eq!(
            motivo("(forall ((x N)) (Parent x @a)))"),
            MotivoAutoria::ParentesisSobrante
        );
        assert_eq!(
            motivo("(forall ((x N)) (Parent x @a)) (Type @a k)"),
            MotivoAutoria::TextoSobrante
        );
        assert_eq!(
            motivo("   ; solo un comentario\n"),
            MotivoAutoria::TextoVacio
        );
        assert_eq!(
            motivo("(forall ((x N)) (HasLab x \"sin cierre))"),
            MotivoAutoria::CadenaSinCerrar
        );
    }

    #[test]
    fn una_constante_mal_formada_se_rechaza_como_error_de_autoria() {
        assert!(matches!(
            motivo("(forall ((x N)) (Parent x @/absoluta))"),
            MotivoAutoria::ConstanteMalFormada { .. }
        ));
        assert_eq!(
            motivo("(forall ((e E)) (= e @sin-flecha))"),
            MotivoAutoria::SorteEquivocadaEnPosicion {
                forma: "=".to_string(),
                posicion: 2,
            }
        );
    }

    #[test]
    fn los_comentarios_no_cambian_lo_que_se_parsea() {
        let con = oracion("; el invariante\n(forall ((x N)) ; liga x\n  (Parent x @rules/index))");
        let sin = oracion("(forall ((x N)) (Parent x @rules/index))");
        assert_eq!(con, sin);
    }

    #[test]
    fn la_forma_canonica_de_una_sentencia_se_vuelve_a_parsear_igual() {
        let canonicos = [
            "(forall ((x N) (y N)) (not (exists ((e E)) (and (Effect x Puro) (Edge e x y delta)))))",
            "(forall ((x N)) :guarda (Type x rule) (Parent x @rules/index))",
            "(exists ((e E)) :guarda (Edge e @a @b rel) (HasLab e \"con \\\"comillas\\\"\"))",
            "(forall ((x N) (y N)) (=> (or (tc Parent x y) (= x y)) (Type x rule)))",
            "(forall ((e E)) (not (= e @a→b#rel#0)))",
            "(forall ((x N)) (Raiz x))",
            "(esquema ((l Lab)) (forall ((x N) (y N)) (=> (and (HasLab x l) (HasLab y l)) (= x y))))",
            "(esquema ((c Dom.Standing)) (forall ((x N)) (Standing x c)))",
        ];
        for texto in canonicos {
            let sentencia = parsear(texto).unwrap_or_else(|e| panic!("{texto}: {e}"));
            assert_eq!(sentencia.to_string(), texto);
            assert_eq!(parsear(&sentencia.to_string()).unwrap(), sentencia);
        }
    }

    #[test]
    fn la_forma_canonica_normaliza_los_blancos_sin_cambiar_el_ast() {
        let suelto = "(forall\n   ((x N))\n   ; comentario\n   (Parent x   @rules/index))";
        let sentencia = parsear(suelto).unwrap();
        assert_eq!(
            sentencia.to_string(),
            "(forall ((x N)) (Parent x @rules/index))"
        );
    }

    /// Una fórmula `(not (not … (Type @a k) …))` con `niveles` listas
    /// abiertas anidadas.
    fn anidada(niveles: usize) -> String {
        format!(
            "{}(Type @a k){}",
            "(not ".repeat(niveles - 1),
            ")".repeat(niveles - 1)
        )
    }

    #[test]
    fn el_anidamiento_excesivo_es_un_error_de_autoria_y_no_un_desborde_de_pila() {
        assert!(matches!(
            motivo(&"(".repeat(200_000)),
            MotivoAutoria::AnidamientoExcesivo { .. }
        ));
        assert!(matches!(
            motivo(&anidada(200_000)),
            MotivoAutoria::AnidamientoExcesivo { .. }
        ));
    }

    #[test]
    fn el_anidamiento_admitido_llega_hasta_el_limite_declarado() {
        assert!(parsear(&anidada(PROFUNDIDAD_MAXIMA)).is_ok());
        assert!(matches!(
            motivo(&anidada(PROFUNDIDAD_MAXIMA + 1)),
            MotivoAutoria::AnidamientoExcesivo { .. }
        ));
    }

    #[test]
    fn una_variable_no_cambia_de_sorte_ni_en_alcances_hermanos() {
        assert_eq!(
            motivo("(and (forall ((x N)) (Type x rule)) (forall ((x E)) (HasLab x \"l\")))"),
            MotivoAutoria::VariableRedeclarada("x".to_string())
        );
    }

    #[test]
    fn una_variable_puede_reaparecer_en_alcances_hermanos_con_la_misma_sorte() {
        // La regla prohíbe cambiar de sorte, no reutilizar el nombre: dos
        // cuantificadores hermanos sobre la misma sorte son legítimos.
        assert!(
            parsear("(and (forall ((x N)) (Type x rule)) (forall ((x N)) (Type x value)))").is_ok()
        );
    }

    #[test]
    fn el_motivo_concreto_de_una_constante_mal_formada_llega_al_autor() {
        let MotivoAutoria::ConstanteMalFormada {
            texto,
            motivo: por_que,
        } = motivo("(forall ((x N)) (Parent x @/absoluta))")
        else {
            panic!("se esperaba ConstanteMalFormada");
        };
        assert_eq!(texto, "@/absoluta");
        assert_eq!(por_que, ErrorPath::Absoluto.to_string());

        let MotivoAutoria::ConstanteMalFormada {
            texto,
            motivo: por_que,
        } = motivo("(forall ((e E)) (= e @a→b#rel#x))")
        else {
            panic!("se esperaba ConstanteMalFormada para el id de arista");
        };
        assert_eq!(texto, "@a→b#rel#x");
        assert!(
            por_que.contains("ordinal"),
            "el motivo debe citar el ordinal inválido: {por_que}"
        );
    }

    #[test]
    fn una_ligadura_no_puede_llamarse_como_una_metavariable_del_esquema() {
        // Sin esta regla, el mismo identificador denota dos ligaduras
        // distintas en el mismo AST y cuál gana depende de la posición
        // sintáctica: `(HasLab l l)` resolvería el sujeto como variable de
        // nodo y la etiqueta como metavariable.
        assert_eq!(
            motivo("(esquema ((l Lab)) (forall ((l N)) (HasLab l l)))"),
            MotivoAutoria::ColisionConMetavariable("l".to_string())
        );
        assert_eq!(
            motivo(
                "(esquema ((c KV)) (forall ((x N)) (exists ((c E)) (and (Type x c) (HasLab c \"\")))))"
            ),
            MotivoAutoria::ColisionConMetavariable("c".to_string())
        );
    }

    #[test]
    fn la_clausura_transitiva_admite_constantes_en_ambas_puntas() {
        let f = oracion("(forall ((x N)) (or (tc Parent x @rules/index) (tc Parent @a/b @c/d)))");
        let Formula::ParaTodo { cuerpo, .. } = f else {
            panic!("se esperaba un ∀");
        };
        let Formula::O(alternativas) = *cuerpo else {
            panic!("se esperaba una disyunción");
        };
        assert_eq!(
            alternativas[0],
            Formula::TcParent {
                desde: TerminoNodo::Var(Var::nuevo("x")),
                hasta: TerminoNodo::Constante(BundlePath::nuevo("rules/index").unwrap()),
            }
        );
        assert_eq!(
            alternativas[1],
            Formula::TcParent {
                desde: TerminoNodo::Constante(BundlePath::nuevo("a/b").unwrap()),
                hasta: TerminoNodo::Constante(BundlePath::nuevo("c/d").unwrap()),
            }
        );
    }

    #[test]
    fn edge_admite_una_constante_de_arista_en_la_primera_posicion() {
        let texto = "(forall ((x N)) (Edge @a→b#rel#0 @a @b rel))";
        let f = oracion(texto);
        let Formula::ParaTodo { cuerpo, .. } = f else {
            panic!("se esperaba un ∀");
        };
        let Formula::Edge { arista, sorte, .. } = *cuerpo else {
            panic!("se esperaba un Edge");
        };
        assert_eq!(
            arista,
            TerminoArista::Constante(IdArista::parsear("a→b#rel#0").unwrap())
        );
        assert_eq!(sorte, TerminoSorte::Literal(Sorte::nuevo("rel")));
        assert_eq!(parsear(texto).unwrap().to_string(), texto);
    }

    #[test]
    fn la_cota_de_anidamiento_aguanta_en_un_hilo_de_pila_reducida() {
        // La cota de `PROFUNDIDAD_MAXIMA` sale de una medición; esto la vuelve
        // a medir en cada corrida, sobre el caso más apretado que documenta:
        // un hilo de 512 KiB, bastante menos que la pila por defecto de
        // un hilo de Rust, con binario de depuración. El margen queda
        // comprobado y no solo enunciado.
        let hilo = std::thread::Builder::new()
            .stack_size(512 * 1024)
            .spawn(|| {
                assert!(parsear(&anidada(PROFUNDIDAD_MAXIMA)).is_ok());
                assert!(matches!(
                    parsear(&anidada(PROFUNDIDAD_MAXIMA + 1))
                        .expect_err("debería rechazar")
                        .motivo,
                    MotivoAutoria::AnidamientoExcesivo { .. }
                ));
                assert!(parsear(&"(".repeat(200_000)).is_err());
            })
            .expect("no se pudo crear el hilo");
        hilo.join().expect("el parser desbordó la pila reducida");
    }

    #[test]
    fn el_error_de_ligadura_senala_el_binder_ofensor_y_no_la_lista_entera() {
        //             1         2         3         4
        //    1234567890123456789012345678901234567890
        let texto = "(esquema ((l Lab)) (forall ((x N) (l N)) (HasLab l l)))";
        assert_eq!(&texto[34..39], "(l N)");
        let error = parsear(texto).expect_err("debería rechazar la colisión");
        assert_eq!(
            error.motivo,
            MotivoAutoria::ColisionConMetavariable("l".to_string())
        );
        assert_eq!((error.linea, error.columna), (1, 35));

        let texto = "(forall ((x N)) (exists ((y E) (x E)) (Edge x @a @b k)))";
        assert_eq!(&texto[31..36], "(x E)");
        let error = parsear(texto).expect_err("debería rechazar la redeclaración");
        assert_eq!(
            error.motivo,
            MotivoAutoria::VariableRedeclarada("x".to_string())
        );
        assert_eq!((error.linea, error.columna), (1, 32));
    }

    #[test]
    fn un_predicado_de_extension_con_la_aridad_equivocada_no_es_una_forma_desconocida() {
        // Todo símbolo de cabeza que no sea una forma reservada es un
        // predicado de extensión, y su aridad es 2: decir «forma
        // desconocida» mandaría al autor a revisar el nombre cuando lo que
        // está mal es la cantidad de argumentos.
        assert!(matches!(
            motivo("(forall ((x N)) (Effect x @a @b))"),
            MotivoAutoria::AridadIncorrecta { forma: ref f, ref esperada, encontrada: 3 }
                if f == "Effect" && esperada == "2"
        ));
        assert!(matches!(
            motivo("(forall ((x N)) (Effect x))"),
            MotivoAutoria::AridadIncorrecta { forma: ref f, encontrada: 1, .. } if f == "Effect"
        ));
        // `FormaDesconocida` sigue siendo el motivo cuando lo que hay en
        // posición de fórmula no es una lista con cabeza de símbolo.
        assert!(matches!(
            motivo("(not x)"),
            MotivoAutoria::FormaDesconocida(_)
        ));
    }

    #[test]
    fn el_mensaje_de_colision_con_metavariable_esta_bien_formado() {
        let texto = MotivoAutoria::ColisionConMetavariable("l".to_string()).to_string();
        assert!(
            !texto.contains("  "),
            "el mensaje tiene espacios duplicados: {texto:?}"
        );
        assert!(
            texto.contains("denotaría") && texto.contains("según"),
            "el mensaje debe estar acentuado como el resto del archivo: {texto:?}"
        );
    }
}
