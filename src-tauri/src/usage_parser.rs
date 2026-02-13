use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindow {
    pub name: String,
    pub utilization: f64,
    pub resets_at: Option<Value>,
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force_exhausted: Option<bool>,
    pub window_seconds: Option<f64>,
}

impl UsageWindow {
    pub fn new(
        name: String,
        utilization: f64,
        resets_at: Option<Value>,
        window_seconds: Option<f64>,
        force_exhausted: bool,
        status: Option<String>,
    ) -> Self {
        Self {
            name,
            utilization,
            resets_at,
            status,
            force_exhausted: if force_exhausted { Some(true) } else { None },
            window_seconds,
        }
    }

    pub fn unknown() -> Self {
        Self {
            name: "(不明な形式)".to_string(),
            utilization: 0.0,
            resets_at: None,
            status: Some("unknown".to_string()),
            force_exhausted: None,
            window_seconds: None,
        }
    }
}

fn get_any<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    let obj = value.as_object()?;
    for key in keys {
        if let Some(v) = obj.get(*key) {
            return Some(v);
        }
    }
    None
}

fn to_number(value: &Value) -> Option<f64> {
    if let Some(n) = value.as_f64() {
        return Some(n);
    }
    value.as_str()?.parse::<f64>().ok()
}

fn normalize_window_name(seconds: Option<f64>, fallback: Option<&str>) -> String {
    if let Some(label) = fallback {
        return label.to_string();
    }
    let sec = seconds.unwrap_or(0.0);
    if (sec - 18000.0).abs() < 1.0 {
        return "5時間".to_string();
    }
    if (sec - 604800.0).abs() < 1.0 {
        return "7日間".to_string();
    }
    if (sec - 86400.0).abs() < 1.0 {
        return "24時間".to_string();
    }
    if sec <= 0.0 {
        return "ウィンドウ".to_string();
    }
    if (sec % 86400.0).abs() < 1.0 {
        return format!("{}日間", (sec / 86400.0).round() as i64);
    }
    format!("{}時間", (sec / 3600.0).round() as i64)
}

pub fn parse_claude_usage(data: &Value) -> Vec<UsageWindow> {
    let preferred = vec![
        ("five_hour", "5時間", 18000.0),
        ("seven_day", "7日間", 604800.0),
        ("seven_day_opus", "7日間 (Opus)", 604800.0),
        ("seven_day_sonnet", "7日間 (Sonnet)", 604800.0),
        ("seven_day_oauth_apps", "7日間 (OAuth Apps)", 604800.0),
        ("seven_day_cowork", "7日間 (Cowork)", 604800.0),
    ];

    let mut windows = Vec::new();
    let mut pushed = HashSet::<String>::new();

    for (key, label, win_sec) in preferred {
        if let Some(block) = data.get(key).and_then(Value::as_object) {
            if let Some(utilization) = block.get("utilization").and_then(to_number) {
                windows.push(UsageWindow::new(
                    label.to_string(),
                    utilization,
                    block.get("resets_at").cloned(),
                    Some(win_sec),
                    false,
                    None,
                ));
                pushed.insert(key.to_string());
            }
        }
    }

    if let Some(obj) = data.as_object() {
        for (key, value) in obj {
            if pushed.contains(key) {
                continue;
            }
            let Some(block) = value.as_object() else {
                continue;
            };
            let Some(utilization) = block.get("utilization").and_then(to_number) else {
                continue;
            };
            let guessed_seconds = if key.starts_with("seven_day") {
                Some(604800.0)
            } else if key.contains("hour") {
                Some(18000.0)
            } else {
                None
            };
            windows.push(UsageWindow::new(
                key.replace('_', " "),
                utilization,
                block.get("resets_at").cloned(),
                guessed_seconds,
                false,
                None,
            ));
        }
    }

    if windows.is_empty() {
        windows.push(UsageWindow::unknown());
    }

    windows
}

fn push_codex_window(
    window_data: &Value,
    label: Option<String>,
    parent: Option<&Value>,
    windows: &mut Vec<UsageWindow>,
) {
    if !window_data.is_object() {
        return;
    }

    let direct_util = get_any(window_data, &["used_percent", "usedPercent", "utilization"]).and_then(to_number);
    let derived_util = {
        let used = get_any(window_data, &["used"]).and_then(to_number);
        let limit = get_any(window_data, &["limit"]).and_then(to_number);
        match (used, limit) {
            (Some(used), Some(limit)) if limit > 0.0 => Some((used / limit) * 100.0),
            _ => None,
        }
    };
    let utilization = direct_util.or(derived_util).unwrap_or(0.0);

    let limit_reached = get_any(window_data, &["limit_reached", "limitReached"]) 
        .and_then(Value::as_bool)
        .or_else(|| {
            parent
                .and_then(|p| get_any(p, &["limit_reached", "limitReached"]))
                .and_then(Value::as_bool)
        });

    let allowed = get_any(window_data, &["allowed"]) 
        .and_then(Value::as_bool)
        .or_else(|| {
            parent
                .and_then(|p| get_any(p, &["allowed"]))
                .and_then(Value::as_bool)
        });

    let force_exhausted = limit_reached == Some(true) || allowed == Some(false);
    let window_seconds = get_any(window_data, &["limit_window_seconds", "limitWindowSeconds"])
        .and_then(to_number);

    let resets_at = get_any(window_data, &["reset_at", "resetAt", "resets_at", "resetsAt"]).cloned();

    windows.push(UsageWindow::new(
        normalize_window_name(window_seconds, label.as_deref()),
        utilization,
        resets_at,
        window_seconds,
        force_exhausted,
        None,
    ));
}

fn parse_wham_rate_limit(block: &Value, prefix: Option<&str>, windows: &mut Vec<UsageWindow>) {
    if !block.is_object() {
        return;
    }

    if let Some(primary) = get_any(block, &["primary_window", "primaryWindow", "primary"]) {
        let label = prefix.map(|p| format!("{p} (primary)"));
        push_codex_window(primary, label, Some(block), windows);
    }

    if let Some(secondary) = get_any(block, &["secondary_window", "secondaryWindow", "secondary"]) {
        let label = prefix.map(|p| format!("{p} (secondary)"));
        push_codex_window(secondary, label, Some(block), windows);
    }
}

pub fn parse_codex_usage(data: &Value) -> Vec<UsageWindow> {
    let mut windows = Vec::new();

    if let Some(rate_limit) = data.get("rate_limit") {
        parse_wham_rate_limit(rate_limit, None, &mut windows);
    }
    if let Some(code_review) = data.get("code_review_rate_limit") {
        parse_wham_rate_limit(code_review, Some("Code Review"), &mut windows);
    }

    if let Some(additional) = data.get("additional_rate_limits") {
        if let Some(arr) = additional.as_array() {
            for (idx, block) in arr.iter().enumerate() {
                let label = block
                    .get("name")
                    .and_then(Value::as_str)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("Additional {}", idx + 1));
                parse_wham_rate_limit(block, Some(&label), &mut windows);
            }
        } else {
            parse_wham_rate_limit(additional, Some("Additional"), &mut windows);
        }
    }

    if windows.is_empty() {
        let fallback_block = data
            .get("rate_limits")
            .or_else(|| data.get("rateLimits"))
            .unwrap_or(data);

        if fallback_block.get("primary").is_some() || fallback_block.get("secondary").is_some() {
            if let Some(primary) = fallback_block.get("primary") {
                push_codex_window(primary, Some("5時間".to_string()), Some(fallback_block), &mut windows);
            }
            if let Some(secondary) = fallback_block.get("secondary") {
                push_codex_window(secondary, Some("7日間".to_string()), Some(fallback_block), &mut windows);
            }
        }
    }

    if windows.is_empty() {
        for key in ["windows", "limits", "rate_limits"] {
            if let Some(arr) = data.get(key).and_then(Value::as_array) {
                for item in arr {
                    let label = get_any(item, &["name", "label", "window"])
                        .and_then(Value::as_str)
                        .map(|s| s.to_string());
                    push_codex_window(item, label, None, &mut windows);
                }
                if !windows.is_empty() {
                    break;
                }
            }
        }
    }

    if windows.is_empty() {
        for (key, label) in [
            ("five_hour", "5時間"),
            ("fiveHour", "5時間"),
            ("weekly", "7日間"),
            ("seven_day", "7日間"),
        ] {
            if let Some(block) = data.get(key) {
                push_codex_window(block, Some(label.to_string()), Some(block), &mut windows);
            }
        }
    }

    if windows.is_empty() {
        windows.push(UsageWindow::unknown());
    }

    windows
}
