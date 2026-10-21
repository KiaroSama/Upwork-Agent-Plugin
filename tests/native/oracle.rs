use serde_json::{Value, json};

pub fn terminal(value: &Value) -> Option<&'static str> {
    let object = value.as_object()?;
    if object.get("jsonrpc")? != "2.0" {
        return None;
    }
    let id = object.get("id")?;
    if !(id.as_i64() == Some(1) || id.as_str() == Some("1"))
        || ["method", "result", "error"]
            .iter()
            .filter(|key| object.contains_key(**key))
            .count()
            != 1
    {
        return None;
    }
    if let Some(error) = object.get("error").and_then(Value::as_object)
        && error.get("code").is_some_and(Value::is_i64)
        && error.get("message").is_some_and(Value::is_string)
    {
        return Some("error");
    }
    object
        .get("result")
        .filter(|v| v.is_object())
        .map(|_| "result")
}

pub fn kind(value: &Value) -> &'static str {
    if let Some(kind) = terminal(value) {
        return kind;
    }
    if value.get("jsonrpc") == Some(&json!("2.0"))
        && value.get("method").is_some_and(Value::is_string)
        && ["id", "result", "error"]
            .iter()
            .all(|key| value.get(key).is_none())
    {
        "notification"
    } else {
        "invalid"
    }
}

#[derive(Default)]
pub struct Observation {
    pub first: Option<&'static str>,
    pub invalid: bool,
}
impl Observation {
    pub fn line(&mut self, line: &[u8]) {
        match serde_json::from_slice::<Value>(line)
            .ok()
            .as_ref()
            .map(kind)
        {
            Some("notification") => {}
            Some(kind @ ("result" | "error")) if self.first.is_none() => self.first = Some(kind),
            _ => self.invalid = true,
        }
    }
}

pub fn controlled_rejection(exit: i32, timed_out: bool) -> bool {
    exit == 1 && !timed_out
}

pub fn rejected(result: &Value) -> bool {
    result["returncode"] == 1
        && result["controller_timeout"] == false
        && result["protocol_invalid"] == false
        && result["first_terminal_message"].is_null()
        && result["transport_rejected"] == true
}

pub fn controls() -> Value {
    for value in [
        Value::Null,
        json!([]),
        json!({}),
        json!({"jsonrpc":"2.0","method":"notice"}),
        json!({"jsonrpc":"2.0","id":99,"result":{}}),
        json!({"jsonrpc":"1.0","id":1,"result":{}}),
        json!({"jsonrpc":"2.0","id":true,"result":{}}),
        json!({"jsonrpc":"2.0","id":1,"result":{},"error":{}}),
        json!({"jsonrpc":"2.0","id":1,"result":{},"method":"ping"}),
    ] {
        assert!(terminal(&value).is_none());
    }
    for id in [json!(1), json!("1")] {
        assert_eq!(
            terminal(&json!({"jsonrpc":"2.0","id":id,"result":{}})),
            Some("result")
        );
        assert_eq!(
            terminal(&json!({"jsonrpc":"2.0","id":id,"error":{"code":-1,"message":"fixture"}})),
            Some("error")
        );
    }
    for error in [
        json!({}),
        json!({"code":true,"message":"fixture"}),
        json!({"code":-1}),
        json!({"code":-1,"message":3}),
    ] {
        assert_eq!(
            kind(&json!({"jsonrpc":"2.0","id":1,"error":error})),
            "invalid"
        );
    }
    assert_eq!(
        kind(&json!({"jsonrpc":"2.0","method":"notifications/progress"})),
        "notification"
    );
    for value in [
        json!({"method":"notice"}),
        json!({"jsonrpc":"1.0","method":"notice"}),
        json!({"jsonrpc":"2.0","id":3,"method":"ping"}),
        json!({"jsonrpc":"2.0","id":1,"result":null}),
    ] {
        assert_eq!(kind(&value), "invalid");
    }
    let base = json!({"returncode":1,"controller_timeout":false,"protocol_invalid":false,
        "first_terminal_message":null,"transport_rejected":true});
    assert!(rejected(&base));
    for (key, value) in [
        ("returncode", json!(0)),
        ("returncode", json!(-9)),
        ("controller_timeout", json!(true)),
        ("protocol_invalid", json!(true)),
        ("transport_rejected", json!(false)),
        ("first_terminal_message", json!("result")),
    ] {
        let mut changed = base.clone();
        changed[key] = value;
        assert!(!rejected(&changed));
    }
    assert!(controlled_rejection(1, false));
    for (exit, timed) in [(0, false), (101, false), (-9, false), (-9, true), (1, true)] {
        assert!(!controlled_rejection(exit, timed));
    }
    let success = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}";
    for (lines, first, invalid) in [
        (vec![success.as_slice()], Some("result"), false),
        (
            vec![success.as_slice(), success.as_slice()],
            Some("result"),
            true,
        ),
        (vec![b"not-json".as_slice()], None, true),
        (vec![b"\xff".as_slice()], None, true),
        (vec![], None, false),
        (
            vec![b"{\"jsonrpc\":\"2.0\",\"id\":99,\"result\":{}}".as_slice()],
            None,
            true,
        ),
    ] {
        let mut observed = Observation::default();
        for line in lines {
            observed.line(line);
        }
        assert_eq!((observed.first, observed.invalid), (first, invalid));
    }
    json!({"passed":true,"tier":"pure-controller","native_executed":false,
        "controls":["identity","envelope","causal-rejection","utf8","duplicate-terminal"]})
}
