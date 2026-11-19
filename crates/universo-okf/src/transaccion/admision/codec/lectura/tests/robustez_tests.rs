use super::*;

#[test]
fn preflight_rechaza_tags_rutas_y_orden_no_canonicos_sin_filtrar_datos() {
    let invalido = ErrorCodecAdmision::FramingInvalido;
    rechaza(
        &wire(
            &[entrada(2, b"a", None)],
            config(b"", None, b"", None, b"{}", b"e", b"p"),
        ),
        invalido,
    );
    rechaza(
        &wire(
            &[directorio(b"../CANARY")],
            config(b"", None, b"", None, b"{}", b"e", b"p"),
        ),
        invalido,
    );
    rechaza(
        &wire(
            &[directorio(&[0xff])],
            config(b"", None, b"", None, b"{}", b"e", b"p"),
        ),
        invalido,
    );
    rechaza(
        &wire(
            &[directorio(b"b"), directorio(b"a")],
            config(b"", None, b"", None, b"{}", b"e", b"p"),
        ),
        invalido,
    );
    rechaza(
        &wire(
            &[directorio(b"a"), directorio(b"a")],
            config(b"", None, b"", None, b"{}", b"e", b"p"),
        ),
        invalido,
    );
    let error = error(&wire(
        &[directorio(b"../CANARY")],
        config(b"", None, b"", None, b"{}", b"e", b"p"),
    ));
    assert!(!format!("{error:?}").contains("CANARY"));
}

#[test]
fn preflight_rechaza_markers_root_desconocidos_en_ambas_configuraciones() {
    let mut manifiesto = fixture_minimo();
    manifiesto[34] = 2;
    rechaza(&manifiesto, ErrorCodecAdmision::FramingInvalido);

    let mut invariantes = fixture_minimo();
    invariantes[43] = 2;
    rechaza(&invariantes, ErrorCodecAdmision::FramingInvalido);
}

#[test]
fn preflight_aplica_cuotas_agregadas_en_el_limite_y_uno_mas() {
    let entries = [directorio(b"a"), archivo(b"b", b"xy")];
    let bytes = wire(
        &entries,
        config(b"m", Some("m"), b"iv", Some("i"), b"{}", b"engine", b"pol"),
    );
    let exactas = CuotasLectura {
        total: bytes.len(),
        entradas: 2,
        arbol_raw: 2,
        configuracion_raw: 3,
        delta: 2,
        rutas_arbol: 2,
        rutas_configuracion: 2,
        profundidad: 1,
        metadata: 9,
    };
    parsear_con_limites(&bytes, exactas).unwrap();
    let excesos = [
        (
            CuotasLectura {
                total: bytes.len() - 1,
                ..exactas
            },
            ErrorCodecAdmision::TotalExcedeLimite,
        ),
        (
            CuotasLectura {
                entradas: 1,
                ..exactas
            },
            ErrorCodecAdmision::LimiteComponenteExcedido,
        ),
        (
            CuotasLectura {
                arbol_raw: 1,
                ..exactas
            },
            ErrorCodecAdmision::LimiteComponenteExcedido,
        ),
        (
            CuotasLectura {
                configuracion_raw: 2,
                ..exactas
            },
            ErrorCodecAdmision::LimiteComponenteExcedido,
        ),
        (
            CuotasLectura {
                delta: 1,
                ..exactas
            },
            ErrorCodecAdmision::LimiteComponenteExcedido,
        ),
        (
            CuotasLectura {
                rutas_arbol: 1,
                ..exactas
            },
            ErrorCodecAdmision::LimiteComponenteExcedido,
        ),
        (
            CuotasLectura {
                rutas_configuracion: 1,
                ..exactas
            },
            ErrorCodecAdmision::LimiteComponenteExcedido,
        ),
        (
            CuotasLectura {
                profundidad: 0,
                ..exactas
            },
            ErrorCodecAdmision::LimiteComponenteExcedido,
        ),
        (
            CuotasLectura {
                metadata: 8,
                ..exactas
            },
            ErrorCodecAdmision::LimiteComponenteExcedido,
        ),
    ];
    for (cuotas, esperado) in excesos {
        assert_eq!(parsear_con_limites(&bytes, cuotas).err(), Some(esperado));
    }
}

#[test]
fn preflight_rechaza_conteo_inconsistente_y_rutas_config_invalidas() {
    let mut demasiadas = fixture_minimo();
    demasiadas[22..26].copy_from_slice(&100_001_u32.to_be_bytes());
    rechaza(&demasiadas, ErrorCodecAdmision::LimiteComponenteExcedido);
    let mut demasiadas_para_el_buffer = fixture_minimo();
    demasiadas_para_el_buffer[22..26].copy_from_slice(&100_000_u32.to_be_bytes());
    rechaza(
        &demasiadas_para_el_buffer,
        ErrorCodecAdmision::FramingInvalido,
    );
    rechaza(
        &wire(
            &[],
            config(b"m", Some("../m"), b"i", None, b"{}", b"e", b"p"),
        ),
        ErrorCodecAdmision::FramingInvalido,
    );
    rechaza(
        &wire(
            &[],
            config(b"m", Some("same"), b"i", Some("same"), b"{}", b"e", b"p"),
        ),
        ErrorCodecAdmision::FramingInvalido,
    );
}

#[test]
fn preflight_rechaza_longitudes_u32_y_u64_enormes_en_buffers_pequenos() {
    let mut ruta = fixture_minimo();
    let mut entrada = vec![0];
    entrada.extend_from_slice(&u32::MAX.to_be_bytes());
    ruta.splice(26..26, entrada);
    ruta[22..26].copy_from_slice(&1_u32.to_be_bytes());
    set_total(&mut ruta);
    rechaza(&ruta, ErrorCodecAdmision::LimiteComponenteExcedido);

    let mut archivo = fixture_minimo();
    let mut entrada = vec![1];
    agregar_u32_bytes(&mut entrada, b"x");
    entrada.extend_from_slice(&u64::MAX.to_be_bytes());
    archivo.splice(26..26, entrada);
    archivo[22..26].copy_from_slice(&1_u32.to_be_bytes());
    set_total(&mut archivo);
    rechaza(&archivo, ErrorCodecAdmision::LimiteComponenteExcedido);

    let mut manifiesto = fixture_minimo();
    manifiesto[26..34].copy_from_slice(&u64::MAX.to_be_bytes());
    rechaza(&manifiesto, ErrorCodecAdmision::LimiteComponenteExcedido);

    let mut delta = fixture_minimo();
    delta[44..52].copy_from_slice(&u64::MAX.to_be_bytes());
    rechaza(&delta, ErrorCodecAdmision::LimiteComponenteExcedido);

    let mut motor = fixture_minimo();
    motor[54..58].copy_from_slice(&u32::MAX.to_be_bytes());
    rechaza(&motor, ErrorCodecAdmision::LimiteComponenteExcedido);

    let mut total = fixture_minimo();
    total[14..22].copy_from_slice(&u64::MAX.to_be_bytes());
    rechaza(&total, ErrorCodecAdmision::TotalExcedeLimite);
}
