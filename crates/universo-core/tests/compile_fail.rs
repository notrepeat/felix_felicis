#[test]
fn nodo_id_y_arista_id_no_son_intercambiables() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/*.rs");
}
