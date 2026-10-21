use super::{
    child::{Result, execute, require},
    fixture::{self, Fixture},
    initialization::{self, MEDIA_CASES, TERMINAL_CASES},
    oracle,
};
use serde_json::{Value, json};
use std::path::Path;

pub const PROTOCOL: [&str; 30] = [
    "valid",
    "wrong_id",
    "wrong_version",
    "missing_version",
    "missing_capabilities",
    "null_info",
    "bad_info",
    "error",
    "log_first",
    "ping_first",
    "string_id",
    "wrong_string_id",
    "missing_info",
    "numeric_info",
    "bad_name",
    "bad_capabilities",
    "null_capabilities",
    "array_capabilities",
    "null_version",
    "empty_version",
    "numeric_version",
    "stateless_version",
    "bad_instructions",
    "optional_metadata",
    "version_2024-11-05",
    "version_2025-03-26",
    "version_2025-06-18",
    "notification_fail",
    "null_result",
    "empty_result",
];
pub const EXTENDED: [&str; 20] = [
    "valid_json_control",
    "mixed_error_json",
    "mixed_error_sse",
    "mixed_error_null_json",
    "mixed_error_null_sse",
    "mixed_method_json",
    "mixed_method_sse",
    "invalid_jsonrpc_json",
    "invalid_jsonrpc_sse",
    "top_array_json",
    "top_array_sse",
    "invalid_utf8_json",
    "invalid_utf8_sse",
    "malformed_then_valid_sse",
    "error_then_valid_sse",
    "invalid_version_then_valid_sse",
    "valid_sse_lf",
    "valid_sse_crlf",
    "valid_sse_comments",
    "oversize_json",
];
pub const PINGS: [&str; 10] = [
    "numeric",
    "string",
    "numeric_string",
    "multiple",
    "log_first",
    "no_ping",
    "ping_then_error",
    "reply_failure",
    "overflow",
    "fallback",
];

pub async fn run(binary: &Path, output: &Path, family: &str, mode: &str) -> Result<Value> {
    let certs = fixture::temporary(output, "protocol-certs-")?;
    fixture::certificates(certs.path()).await?;
    let cases: &[&str] = match family {
        "protocol" => &PROTOCOL,
        "extended" => &EXTENDED,
        "terminal" => &TERMINAL_CASES,
        "media" => &MEDIA_CASES,
        "ping" => &PINGS,
        _ => return Err("Unknown native protocol suite".into()),
    };
    let modes: Vec<&str> = if matches!(family, "media" | "ping") {
        vec!["sign-in", "bridge"]
    } else {
        vec![mode]
    };
    let mut results = Vec::new();
    for mode in modes {
        for case in cases {
            println!("family={family} mode={mode} case={case} started");
            let root = fixture::temporary(output, "protocol-case-")?;
            let server = Fixture::start(
                certs.path(),
                family,
                if mode == "bridge" { "prepare" } else { case },
            )
            .await?;
            server.seed(root.path())?;
            let result = async {
                if mode == "bridge" {
                    let prepared =
                        execute(binary, root.path(), certs.path(), "sign-in", None).await?;
                    require(
                        prepared["returncode"] == 0 && prepared["completion_claimed"] == true,
                        "Synthetic credential preparation failed",
                    )?;
                    server.state.set_case(case);
                }
                let mut value = execute(
                    binary,
                    root.path(),
                    certs.path(),
                    mode,
                    if family == "ping" && mode == "bridge" {
                        Some(&server.state)
                    } else {
                        None
                    },
                )
                .await?;
                let positive = match family {
                    "protocol" => matches!(
                        *case,
                        "valid"
                            | "log_first"
                            | "ping_first"
                            | "string_id"
                            | "optional_metadata"
                            | "version_2024-11-05"
                            | "version_2025-03-26"
                            | "version_2025-06-18"
                    ),
                    "extended" => matches!(
                        *case,
                        "valid_json_control"
                            | "valid_sse_lf"
                            | "valid_sse_crlf"
                            | "valid_sse_comments"
                    ),
                    "terminal" => matches!(*case, "json_success" | "success_then_error_sse"),
                    "media" => initialization::media_positive(case),
                    "ping" => !matches!(*case, "reply_failure" | "overflow" | "ping_then_error"),
                    _ => false,
                };
                let accepted = if mode == "sign-in" {
                    value["completion_claimed"] == true
                } else {
                    value["first_terminal_message"] == "result"
                };
                let d = server.state.data.lock().unwrap();
                let mut passed = value["controller_timeout"] == false
                    && value["protocol_invalid"] == false
                    && value["private_marker_in_stderr"] == false;
                match family {
                    "protocol" | "extended" => {
                        passed &= value["callback_sent"] == true
                            && (value["returncode"] == 0) == positive
                            && accepted == positive
                            && d.initialized
                                == usize::from(positive || *case == "notification_fail")
                    }
                    "terminal" => {
                        passed &= if mode == "sign-in" {
                            value["callback_sent"] == true
                                && (value["returncode"] == 0) == positive
                                && accepted == positive
                                && d.initialized == usize::from(positive)
                        } else {
                            value["first_terminal_message"]
                                == (if positive { "result" } else { "error" })
                                && (!positive || value["returncode"] == 0)
                        };
                    }
                    "media" => {
                        passed &= if positive {
                            accepted && value["returncode"] == 0
                        } else {
                            oracle::rejected(&value)
                        };
                        passed &= d.responses == 1;
                        if mode == "sign-in" {
                            passed &= value["callback_sent"] == true
                                && (value["returncode"] == 0) == positive
                                && d.initialized == usize::from(positive);
                        }
                        value["response_bytes"] = json!(d.response_bytes);
                    }
                    "ping" => {
                        let expected = match *case {
                            "no_ping" => 0,
                            "multiple" => 2,
                            "overflow" => 16,
                            _ => 1,
                        };
                        passed &= accepted == positive
                            && d.valid
                            && d.pings == expected
                            && d.initializations == 1
                            && d.initialized == usize::from(positive)
                            && d.ready_version_ok
                            && d.flags.get("ready").copied().unwrap_or(false) == positive;
                        if mode == "sign-in" {
                            passed &= value["callback_sent"] == true
                                && (value["returncode"] == 0) == positive;
                        }
                        if *case == "ping_then_error" && mode == "bridge" {
                            passed &= value["first_terminal_message"] == "error";
                        }
                        value["valid_reply_ids_and_headers"] = json!(d.valid);
                        value["negotiated_ready_header_valid"] = json!(d.ready_version_ok);
                    }
                    _ => {}
                }
                value["case"] = json!(case);
                value["mode"] = json!(mode);
                value["passed"] = json!(passed);
                value["expected_success"] = json!(positive);
                value["initialized_sent"] = json!(d.initialized);
                value["ping_replies"] = json!(d.pings);
                value["initialization_requests"] = json!(d.initializations);
                Ok::<_, Box<dyn std::error::Error + Send + Sync>>(value)
            }
            .await;
            server.close().await?;
            let result = result?;
            println!(
                "family={family} mode={mode} case={case} passed={}",
                result["passed"]
            );
            results.push(result);
        }
    }
    Ok(
        json!({"cases":results.len(),"passed":results.iter().filter(|r|r["passed"]==true).count(),"results":results}),
    )
}
