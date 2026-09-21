# Contribuir a felix_felicis

Las mejoras son más valiosas cuando vuelven al proyecto común. Si corriges un
defecto, amplías el motor o encuentras una limitación del modelo, abre un issue
o un pull request con:

1. el problema observado y un caso mínimo reproducible;
2. la decisión de diseño y sus límites;
3. pruebas que fallen antes del cambio y pasen después;
4. documentación actualizada cuando cambie un contrato.

Antes de enviar cambios ejecuta:

```powershell
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
python tools/check_public_boundary.py
```

No adjuntes bundles reales, transcripciones, evidencias ni información de
usuarios. Construye una reproducción ficticia y mínima.

Al contribuir aceptas que tu aporte se distribuya bajo
`AGPL-3.0-or-later`, la licencia del proyecto.
