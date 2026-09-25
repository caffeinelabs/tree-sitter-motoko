use mo_fmt::{Config, format};

#[test]
fn fixtures() {
    insta::glob!("format/**/*.mo", |path| {
        let source = std::fs::read_to_string(path).unwrap();
        let config = Config::default();
        let out = format(&source, &config).unwrap();
        assert_eq!(format(&out, &config).unwrap(), out, "not a fixed point");
        insta::assert_snapshot!(out);
    });
}
