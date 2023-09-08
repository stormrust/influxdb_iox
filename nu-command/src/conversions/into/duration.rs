use nu_engine::CallExt;
use nu_parser::{parse_unit_value, DURATION_UNIT_GROUPS};
use nu_protocol::{
    ast::{Call, CellPath, Expr},
    engine::{Command, EngineState, Stack},
    Category, Example, PipelineData, ShellError, Signature, Span, SyntaxShape, Type, Unit, Value,
};

const NS_PER_SEC: i64 = 1_000_000_000;
#[derive(Clone)]
pub struct SubCommand;

impl Command for SubCommand {
    fn name(&self) -> &str {
        "into duration"
    }

    fn signature(&self) -> Signature {
        Signature::build("into duration")
            .input_output_types(vec![
                (Type::String, Type::Duration),
                (Type::Duration, Type::Duration),
                (Type::Table(vec![]), Type::Table(vec![])),
                //todo: record<hour,minute,sign> | into duration -> Duration
                //(Type::Record(vec![]), Type::Record(vec![])),
            ])
            //.allow_variants_without_examples(true)
            .rest(
                "rest",
                SyntaxShape::CellPath,
                "for a data structure input, convert data at the given cell paths",
            )
            .category(Category::Conversions)
    }

    fn usage(&self) -> &str {
        "Convert value to duration."
    }

    fn extra_usage(&self) -> &str {
        "Max duration value is i64::MAX nanoseconds; max duration time unit is wk (weeks)."
    }

    fn search_terms(&self) -> Vec<&str> {
        vec!["convert", "time", "period"]
    }

    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        into_duration(engine_state, stack, call, input)
    }

    fn examples(&self) -> Vec<Example> {
        let span = Span::test_data();
        vec![
            Example {
                description: "Convert duration string to duration value",
                example: "'7min' | into duration",
                result: Some(Value::Duration {
                    val: 7 * 60 * NS_PER_SEC,
                    span,
                }),
            },
            Example {
                description: "Convert compound duration string to duration value",
                example: "'1day 2hr 3min 4sec' | into duration",
                result: Some(Value::Duration {
                    val: (((((/* 1 * */24) + 2) * 60) + 3) * 60 + 4) * NS_PER_SEC,
                    span,
                }),
            },
            Example {
                description: "Convert table of duration strings to table of duration values",
                example:
                    "[[value]; ['1sec'] ['2min'] ['3hr'] ['4day'] ['5wk']] | into duration value",
                result: Some(Value::List {
                    vals: vec![
                        Value::Record {
                            cols: vec!["value".to_string()],
                            vals: vec![Value::Duration {
                                val: NS_PER_SEC,
                                span,
                            }],
                            span,
                        },
                        Value::Record {
                            cols: vec!["value".to_string()],
                            vals: vec![Value::Duration {
                                val: 2 * 60 * NS_PER_SEC,
                                span,
                            }],
                            span,
                        },
                        Value::Record {
                            cols: vec!["value".to_string()],
                            vals: vec![Value::Duration {
                                val: 3 * 60 * 60 * NS_PER_SEC,
                                span,
                            }],
                            span,
                        },
                        Value::Record {
                            cols: vec!["value".to_string()],
                            vals: vec![Value::Duration {
                                val: 4 * 24 * 60 * 60 * NS_PER_SEC,
                                span,
                            }],
                            span,
                        },
                        Value::Record {
                            cols: vec!["value".to_string()],
                            vals: vec![Value::Duration {
                                val: 5 * 7 * 24 * 60 * 60 * NS_PER_SEC,
                                span,
                            }],
                            span,
                        },
                    ],
                    span,
                }),
            },
            Example {
                description: "Convert duration to duration",
                example: "420sec | into duration",
                result: Some(Value::Duration {
                    val: 7 * 60 * NS_PER_SEC,
                    span,
                }),
            },
        ]
    }
}

fn into_duration(
    engine_state: &EngineState,
    stack: &mut Stack,
    call: &Call,
    input: PipelineData,
) -> Result<PipelineData, ShellError> {
    let span = match input.span() {
        Some(t) => t,
        None => call.head,
    };
    let column_paths: Vec<CellPath> = call.rest(engine_state, stack, 0)?;

    input.map(
        move |v| {
            if column_paths.is_empty() {
                action(&v, span)
            } else {
                let mut ret = v;
                for path in &column_paths {
                    let r =
                        ret.update_cell_path(&path.members, Box::new(move |old| action(old, span)));
                    if let Err(error) = r {
                        return Value::Error {
                            error: Box::new(error),
                        };
                    }
                }

                ret
            }
        },
        engine_state.ctrlc.clone(),
    )
}

// convert string list of duration values to duration NS.
// technique for getting substrings and span based on: https://stackoverflow.com/a/67098851/2036651
#[inline]
fn addr_of(s: &str) -> usize {
    s.as_ptr() as usize
}

fn split_whitespace_indices(s: &str, span: Span) -> impl Iterator<Item = (&str, Span)> {
    s.split_whitespace().map(move |sub| {
        let start_offset = span.start + addr_of(sub) - addr_of(s);
        (sub, Span::new(start_offset, start_offset + sub.len()))
    })
}

fn compound_to_duration(s: &str, span: Span) -> Result<i64, ShellError> {
    let mut duration_ns: i64 = 0;

    for (substring, substring_span) in split_whitespace_indices(s, span) {
        let sub_ns = string_to_duration(substring, substring_span)?;
        duration_ns += sub_ns;
    }

    Ok(duration_ns)
}

fn string_to_duration(s: &str, span: Span) -> Result<i64, ShellError> {
    if let Some(Ok(expression)) = parse_unit_value(
        s.as_bytes(),
        span,
        DURATION_UNIT_GROUPS,
        Type::Duration,
        |x| x,
    ) {
        if let Expr::ValueWithUnit(value, unit) = expression.expr {
            if let Expr::Int(x) = value.expr {
                match unit.item {
                    Unit::Nanosecond => return Ok(x),
                    Unit::Microsecond => return Ok(x * 1000),
                    Unit::Millisecond => return Ok(x * 1000 * 1000),
                    Unit::Second => return Ok(x * NS_PER_SEC),
                    Unit::Minute => return Ok(x * 60 * NS_PER_SEC),
                    Unit::Hour => return Ok(x * 60 * 60 * NS_PER_SEC),
                    Unit::Day => return Ok(x * 24 * 60 * 60 * NS_PER_SEC),
                    Unit::Week => return Ok(x * 7 * 24 * 60 * 60 * NS_PER_SEC),
                    _ => {}
                }
            }
        }
    }

    Err(ShellError::CantConvertToDuration {
        details: s.to_string(),
        dst_span: span,
        src_span: span,
        help: Some("supported units are ns, us/µs, ms, sec, min, hr, day, and wk".to_string()),
    })
}

fn action(input: &Value, span: Span) -> Value {
    match input {
        Value::Duration { .. } => input.clone(),
        Value::String {
            val,
            span: value_span,
        } => match compound_to_duration(val, *value_span) {
            Ok(val) => Value::Duration { val, span },
            Err(error) => Value::Error {
                error: Box::new(error),
            },
        },
        // Propagate errors by explicitly matching them before the final case.
        Value::Error { .. } => input.clone(),
        other => Value::Error {
            error: Box::new(ShellError::OnlySupportsThisInputType {
                exp_input_type: "string or duration".into(),
                wrong_type: other.get_type().to_string(),
                dst_span: span,
                src_span: other.expect_span(),
            }),
        },
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rstest::rstest;

    #[test]
    fn test_examples() {
        use crate::test_examples;

        test_examples(SubCommand {})
    }

    const NS_PER_SEC: i64 = 1_000_000_000;

    #[rstest]
    #[case("3ns", 3)]
    #[case("4us", 4*1000)]
    #[case("4\u{00B5}s", 4*1000)] // micro sign
    #[case("4\u{03BC}s", 4*1000)] // mu symbol
    #[case("5ms", 5 * 1000 * 1000)]
    #[case("1sec", 1 * NS_PER_SEC)]
    #[case("7min", 7 * 60 * NS_PER_SEC)]
    #[case("42hr", 42 * 60 * 60 * NS_PER_SEC)]
    #[case("123day", 123 * 24 * 60 * 60 * NS_PER_SEC)]
    #[case("3wk", 3 * 7 * 24 * 60 * 60 * NS_PER_SEC)]
    #[case("86hr 26ns", 86 * 3600 * NS_PER_SEC + 26)] // compound duration string
    #[case("14ns 3hr 17sec", 14 + 3 * 3600 * NS_PER_SEC + 17 * NS_PER_SEC)] // compound string with units in random order

    fn turns_string_to_duration(#[case] phrase: &str, #[case] expected_duration_val: i64) {
        let actual = action(&Value::test_string(phrase), Span::new(0, phrase.len()));
        match actual {
            Value::Duration {
                val: observed_val, ..
            } => {
                assert_eq!(expected_duration_val, observed_val, "expected != observed")
            }
            other => {
                panic!("Expected Value::Duration, observed {other:?}");
            }
        }
    }
}
