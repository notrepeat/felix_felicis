//! Fase 10, U5: `universo reglas aplicar`, `manifiesto aplicar` y `seal` con
//! cambio externo, vistos desde `log` y `diff`.
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

struct Salida {
    codigo: i32,
    stdout: String,
    stderr: String,
}

fn universo_en(cwd: &Path, args: &[&str]) -> Salida {
    let out = Command::new(env!("CARGO_BIN_EXE_universo"))
        .args(args)
        .current_dir(cwd)
        .env_remove("FELIX_CRITERIO")
        .env_remove("FELIX_UNIVERSO")
        .output()
        .unwrap();
    Salida {
        codigo: out.status.code().unwrap(),
        stdout: String::from_utf8(out.stdout).unwrap(),
        stderr: String::from_utf8(out.stderr).unwrap(),
    }
}

fn ok(s: &Salida) {
    assert_eq!(s.codigo, 0, "{}{}", s.stdout, s.stderr);
}

fn repo(dir: &Path) -> PathBuf {
    let repo = dir.join("mi-criterio");
    ok(&universo_en(
        dir,
        &["init", repo.to_str().unwrap(), "--git"],
    ));
    ok(&universo_en(
        &repo,
        &[
            "capturar",
            "Regla firme",
            "--proyecto",
            "Alfa",
            "--fecha",
            "2026-11-18",
        ],
    ));
    repo
}

fn reglas(repo: &Path) -> PathBuf {
    repo.join("universos/candidatos/invariants.sx")
}

const CANDIDATAS: &str = "\n(regla :id \"solo-candidatas\" :mensaje \"Todo criterio aquí es candidato.\"\n  (forall ((x N)) (=> (Type x criterio) (Estado x candidato))))\n";
const VIGENTES: &str = "\n(regla :id \"solo-vigentes\" :mensaje \"Todo criterio aquí es vigente.\"\n  (forall ((x N)) (=> (Type x criterio) (Estado x vigente))))\n";

#[test]
fn reglas_aplicar_instala_lo_que_se_cumple_y_rechaza_lo_que_no() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    let previas = fs::read_to_string(reglas(&repo)).unwrap();

    // Más estricta de lo que el bundle cumple: rechazo, nada escrito.
    let estrictas = dir.path().join("estrictas.sx");
    fs::write(&estrictas, format!("{previas}{VIGENTES}")).unwrap();
    let s = universo_en(
        &repo,
        &[
            "reglas",
            "aplicar",
            estrictas.to_str().unwrap(),
            "--format=json",
        ],
    );
    assert_eq!(s.codigo, 1, "{}{}", s.stdout, s.stderr);
    let json: Value = serde_json::from_str(&s.stdout).unwrap();
    assert_eq!(json["status"], "reject");
    assert_eq!(
        json["failed_rules"][0]["rule_id"], "solo-vigentes",
        "{json}"
    );
    assert_eq!(fs::read_to_string(reglas(&repo)).unwrap(), previas);
    let texto = universo_en(&repo, &["reglas", "aplicar", estrictas.to_str().unwrap()]);
    assert!(
        texto
            .stdout
            .contains("rule: solo-vigentes — Todo criterio aquí es vigente."),
        "{}",
        texto.stdout
    );

    // Una que se cumple: instalada, sellada y en el historial.
    let nuevas = dir.path().join("nuevas.sx");
    fs::write(&nuevas, format!("{previas}{CANDIDATAS}")).unwrap();
    let s = universo_en(
        &repo,
        &[
            "reglas",
            "aplicar",
            nuevas.to_str().unwrap(),
            "--autor",
            "ana",
            "--motivo",
            "acotar candidatos",
        ],
    );
    ok(&s);
    assert!(s.stdout.contains("reglas aplicar: applied"), "{}", s.stdout);
    assert!(s.stdout.contains("sealed: true"), "{}", s.stdout);
    assert_eq!(
        fs::read_to_string(reglas(&repo)).unwrap(),
        format!("{previas}{CANDIDATAS}")
    );
    ok(&universo_en(&repo, &["guard"]));
    let log = universo_en(&repo, &["log"]);
    ok(&log);
    assert!(
        log.stdout
            .contains("kind=config author=ana sealed=yes reason=acotar candidatos"),
        "{}",
        log.stdout
    );
    // diff cruza el cambio de reglas sin ver un hueco.
    let d = universo_en(&repo, &["diff", "0"]);
    ok(&d);
    assert!(
        d.stdout.contains("config changed (entry 2)"),
        "{}",
        d.stdout
    );
    let d = universo_en(&repo, &["diff", "0", "--format=json"]);
    let json: Value = serde_json::from_str(&d.stdout).unwrap();
    assert_eq!(json["config_changes"], serde_json::json!([2]));
    // Y lo que viene después sigue funcionando.
    ok(&universo_en(
        &repo,
        &[
            "capturar",
            "Otra regla",
            "--proyecto",
            "Beta",
            "--fecha",
            "2026-11-19",
        ],
    ));
    ok(&universo_en(&repo, &["diff", "0"]));
}

#[test]
fn manifiesto_aplicar_con_dry_run_no_escribe() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    let ruta = repo.join("universos/candidatos/manifest.toml");
    let previo = fs::read_to_string(&ruta).unwrap();
    let nuevo = dir.path().join("manifest.toml");
    fs::write(&nuevo, &previo).unwrap();
    let s = universo_en(
        &repo,
        &[
            "manifiesto",
            "aplicar",
            nuevo.to_str().unwrap(),
            "--dry-run",
            "--format=json",
        ],
    );
    ok(&s);
    let json: Value = serde_json::from_str(&s.stdout).unwrap();
    assert_eq!(json["status"], "admit");
    assert_eq!(json["dry_run"], true);
    let log = universo_en(&repo, &["log", "--format=json"]);
    let json: Value = serde_json::from_str(&log.stdout).unwrap();
    assert_eq!(json["entries"].as_array().unwrap().len(), 1);
    assert_eq!(json["entries"][0]["kind"], "apply");
    // Un archivo que no existe es un error, no un rechazo.
    let s = universo_en(&repo, &["manifiesto", "aplicar", "no-existe.toml"]);
    assert_eq!(s.codigo, 1);
    assert!(s.stdout.contains("config_file_unreadable"), "{}", s.stdout);
    // Sin archivo: uso inválido.
    assert_eq!(universo_en(&repo, &["manifiesto", "aplicar"]).codigo, 2);
}

#[test]
fn sellar_una_edicion_a_mano_queda_como_cambio_externo() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    let nota = repo.join("universos/candidatos/bundle/criterios/regla-firme.md");
    let texto = fs::read_to_string(&nota).unwrap();
    fs::write(&nota, format!("{texto}\nAñadido a mano.\n")).unwrap();
    assert_eq!(universo_en(&repo, &["guard"]).codigo, 1);
    // Cambiar las reglas no acepta la edición en silencio.
    let reglas_actuales = dir.path().join("mismas.sx");
    fs::copy(reglas(&repo), &reglas_actuales).unwrap();
    let s = universo_en(
        &repo,
        &["reglas", "aplicar", reglas_actuales.to_str().unwrap()],
    );
    assert_eq!(s.codigo, 1);
    assert!(s.stdout.contains("config_seal_mismatch"), "{}", s.stdout);
    assert!(s.stdout.contains("universo seal"), "{}", s.stdout);
    let s = universo_en(&repo, &["seal", "--motivo", "revisado a mano"]);
    ok(&s);
    assert!(s.stdout.contains("cambio externo"), "{}", s.stdout);
    let log = universo_en(&repo, &["log"]);
    assert!(
        log.stdout
            .contains("kind=seal author=none sealed=yes reason=revisado a mano"),
        "{}",
        log.stdout
    );
    let d = universo_en(&repo, &["diff", "0"]);
    assert_eq!(d.codigo, 1);
    assert!(d.stdout.contains("external_change"), "{}", d.stdout);
    assert!(d.stdout.contains("entry: 2"), "{}", d.stdout);
    // Desde el sello en adelante, sí.
    ok(&universo_en(&repo, &["diff", "2"]));
    // Sellar de nuevo el mismo estado no añade entradas.
    let s = universo_en(&repo, &["seal", "--format=json"]);
    let json: Value = serde_json::from_str(&s.stdout).unwrap();
    assert_eq!(json["external_change_recorded"], false);
}

#[test]
fn fuera_de_un_repositorio_se_dan_las_rutas() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repo(dir.path());
    let u = repo.join("universos/candidatos");
    let previas = fs::read_to_string(u.join("invariants.sx")).unwrap();
    let nuevas = dir.path().join("nuevas.sx");
    fs::write(&nuevas, format!("{previas}{CANDIDATAS}")).unwrap();
    let manifiesto = format!("--manifiesto={}", u.join("manifest.toml").display());
    let invariantes = format!("--invariantes={}", u.join("invariants.sx").display());
    let s = universo_en(
        dir.path(),
        &[
            "reglas",
            u.join("bundle").to_str().unwrap(),
            "aplicar",
            nuevas.to_str().unwrap(),
            &manifiesto,
            &invariantes,
        ],
    );
    ok(&s);
    assert!(s.stdout.contains("applied"), "{}", s.stdout);
}
