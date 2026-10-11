use crate::Atomo;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LadoDelta {
    Add,
    Remove,
}

impl std::fmt::Display for LadoDelta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LadoDelta::Add => write!(f, "add"),
            LadoDelta::Remove => write!(f, "remove"),
        }
    }
}

/// Delta transaccional explícito: los átomos a añadir y retirar.
///
/// Ambos conjuntos se guardan como vectores tipados en el orden total
/// canónico de `Atomo`; nunca se infieren operaciones a partir de un estado.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Delta {
    add: Vec<Atomo>,
    remove: Vec<Atomo>,
}

/// Error al construir un delta transaccional.
#[derive(Clone, PartialEq, Eq)]
pub enum ErrorDelta {
    /// Un átomo tipado no tiene una representación canónica reversible.
    AtomoNoRepresentable { lado: LadoDelta, indice: usize },
    /// Un mismo átomo canónico aparece en ambos lados del parche.
    Solapamiento,
}

impl Delta {
    /// Ordena y deduplica cada conjunto tipado, y rechaza su intersección.
    pub fn nuevo(
        add: impl IntoIterator<Item = Atomo>,
        remove: impl IntoIterator<Item = Atomo>,
    ) -> Result<Self, ErrorDelta> {
        let add = canonicalizar(add, LadoDelta::Add)?;
        let remove = canonicalizar(remove, LadoDelta::Remove)?;

        if add.iter().any(|atomo| remove.binary_search(atomo).is_ok()) {
            return Err(ErrorDelta::Solapamiento);
        }

        Ok(Self { add, remove })
    }

    /// Átomos a añadir, ordenados y sin duplicados.
    pub fn agregar(&self) -> &[Atomo] {
        &self.add
    }

    /// Átomos a retirar, ordenados y sin duplicados.
    pub fn retirar(&self) -> &[Atomo] {
        &self.remove
    }
}

fn canonicalizar(
    atomos: impl IntoIterator<Item = Atomo>,
    lado: LadoDelta,
) -> Result<Vec<Atomo>, ErrorDelta> {
    let mut canonicales = Vec::new();
    for (indice, atomo) in atomos.into_iter().enumerate() {
        let texto = atomo.to_string();
        if !matches!(Atomo::parsear(&texto), Ok(parseado) if parseado == atomo) {
            return Err(ErrorDelta::AtomoNoRepresentable { lado, indice });
        }
        canonicales.push(atomo);
    }
    canonicales.sort_unstable();
    canonicales.dedup();
    Ok(canonicales)
}

impl std::fmt::Debug for Delta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Delta")
            .field("add_count", &self.add.len())
            .field("remove_count", &self.remove.len())
            .finish()
    }
}

impl std::fmt::Display for ErrorDelta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorDelta::AtomoNoRepresentable { lado, indice } => write!(
                f,
                "el átomo en {lado}[{indice}] no tiene representación canónica reversible"
            ),
            ErrorDelta::Solapamiento => write!(f, "add y remove se solapan"),
        }
    }
}

impl std::error::Error for ErrorDelta {}

impl std::fmt::Debug for ErrorDelta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorDelta::AtomoNoRepresentable { lado, indice } => f
                .debug_struct("AtomoNoRepresentable")
                .field("lado", lado)
                .field("indice", indice)
                .finish(),
            ErrorDelta::Solapamiento => f.write_str("Solapamiento"),
        }
    }
}
