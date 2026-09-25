use mo_fmt::{Config, Syntax, format};
use serde::Deserialize;

#[derive(Deserialize)]
struct Cases {
    case: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    why: String,
    #[serde(default)]
    syntax: Syntax,
    source: String,
    output: String,
}

#[test]
fn every_case_formats_as_expected_and_is_a_fixed_point() {
    let cases: Cases = toml::from_str(include_str!("cases.toml")).unwrap();
    let mut failures = Vec::new();
    for case in &cases.case {
        let config = Config {
            syntax: case.syntax,
            ..Config::default()
        };
        let check = |source: &str, config: &Config, what: &str| match format(source, config) {
            Ok(out) if out == case.output => None,
            Ok(out) => Some(format!(
                "{what}\n--- expected\n{}--- got\n{out}",
                case.output
            )),
            Err(e) => Some(format!("{what}: {e}")),
        };
        let failure = check(&case.source, &config, "formats as expected")
            .or_else(|| check(&case.output, &config, "is a fixed point"))
            .or_else(|| {
                let preserve = Config {
                    syntax: Syntax::Preserve,
                    ..config.clone()
                };
                (case.syntax == Syntax::Moc2)
                    .then(|| check(&case.output, &preserve, "is a fixed point of preserve"))?
            });
        if let Some(f) = failure {
            failures.push(format!("{}: {f}", case.why));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases failed:\n\n{}",
        failures.len(),
        cases.case.len(),
        failures.join("\n\n")
    );
}
