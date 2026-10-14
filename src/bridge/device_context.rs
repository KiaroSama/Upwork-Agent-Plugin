use rmcp::model::{ClientJsonRpcMessage, GetMeta};
use serde_json::{Map, Value};

fn normalize_hints(hints: &mut Map<String, Value>) {
    for key in ["timezone", "timeZone"] {
        if let Some(value) = hints.get_mut(key) {
            *value = Value::String("UTC".into());
        }
    }
    for key in [
        "timezone_offset_minutes",
        "utcOffset",
        "utc_offset",
        "offsetMinutes",
    ] {
        if let Some(value) = hints.get_mut(key) {
            *value = Value::from(0);
        }
    }
}

fn normalize_meta(meta: &mut Map<String, Value>) {
    normalize_hints(meta);
    if let Some(location) = meta
        .get_mut("openai/userLocation")
        .and_then(Value::as_object_mut)
    {
        normalize_hints(location);
    }
}

pub(super) fn normalize(message: &mut ClientJsonRpcMessage) {
    // The SDK moves params._meta into envelope extensions while decoding.
    // Never create absent metadata or recurse into tool arguments/business dates.
    match message {
        ClientJsonRpcMessage::Request(request) if !request.request.get_meta().is_empty() => {
            normalize_meta(request.request.get_meta_mut());
        }
        ClientJsonRpcMessage::Notification(notification)
            if !notification.notification.get_meta().is_empty() =>
        {
            normalize_meta(notification.notification.get_meta_mut());
        }
        _ => {}
    }
}
