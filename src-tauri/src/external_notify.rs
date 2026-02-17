use crate::error::{AppError, AppResult};
use crate::store_repo::read_store;
use std::time::Duration;
use tauri::AppHandle;

const EXTERNAL_NOTIFY_TIMEOUT_SECS: u64 = 10;

const DISCORD_COLOR_OK: u32 = 0x57F287;
const DISCORD_COLOR_WARNING: u32 = 0xFEE75C;
const DISCORD_COLOR_CRITICAL: u32 = 0xED4245;
const DISCORD_COLOR_DEFAULT: u32 = 0x5865F2;

fn discord_embed_color(level: &str) -> u32 {
    match level {
        "ok" => DISCORD_COLOR_OK,
        "warning" => DISCORD_COLOR_WARNING,
        "critical" | "exhausted" => DISCORD_COLOR_CRITICAL,
        _ => DISCORD_COLOR_DEFAULT,
    }
}

fn pushover_priority(level: &str) -> i32 {
    match level {
        "critical" | "exhausted" => 1,
        _ => 0,
    }
}

async fn send_discord(webhook_url: &str, title: &str, body: &str, level: &str) -> AppResult<()> {
    let color = discord_embed_color(level);
    let payload = serde_json::json!({
        "embeds": [{
            "title": title,
            "description": body,
            "color": color,
            "footer": { "text": crate::APP_NAME },
            "timestamp": chrono::Utc::now().to_rfc3339()
        }]
    });

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(EXTERNAL_NOTIFY_TIMEOUT_SECS))
        .build()
        .map_err(|e| AppError::Message(format!("Failed to build HTTP client: {e}")))?;

    let resp = client
        .post(webhook_url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| AppError::Message(format!("Discord request failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(AppError::Message(format!(
            "Discord returned HTTP {}",
            resp.status()
        )));
    }

    Ok(())
}

async fn send_pushover(
    api_token: &str,
    user_key: &str,
    title: &str,
    body: &str,
    level: &str,
) -> AppResult<()> {
    let priority = pushover_priority(level);
    let params = [
        ("token", api_token),
        ("user", user_key),
        ("title", title),
        ("message", body),
    ];
    let mut form = params
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect::<Vec<_>>();
    form.push(("priority".to_string(), priority.to_string()));

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(EXTERNAL_NOTIFY_TIMEOUT_SECS))
        .build()
        .map_err(|e| AppError::Message(format!("Failed to build HTTP client: {e}")))?;

    let resp = client
        .post("https://api.pushover.net/1/messages.json")
        .form(&form)
        .send()
        .await
        .map_err(|e| AppError::Message(format!("Pushover request failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(AppError::Message(format!(
            "Pushover returned HTTP {}",
            resp.status()
        )));
    }

    Ok(())
}

pub async fn send_external_notification(
    app: AppHandle,
    payload: crate::SendExternalNotificationPayload,
) -> AppResult<crate::ExternalNotifyResult> {
    let store = read_store(&app)?;
    let settings = &store.settings.external_notify;
    let title = crate::sanitize_string(payload.title.as_deref(), "");
    let body = crate::sanitize_string(payload.body.as_deref(), "");
    let level = crate::sanitize_string(payload.level.as_deref(), "");
    let channel = crate::sanitize_string(payload.channel.as_deref(), "");

    if title.is_empty() {
        return Err(AppError::InvalidInput("title is required".to_string()));
    }

    let mut errors = Vec::new();
    let send_all = channel.is_empty();

    // Discord
    let should_send_discord = if send_all {
        settings.discord.enabled && !settings.discord.webhook_url.is_empty()
    } else {
        channel == "discord" && !settings.discord.webhook_url.is_empty()
    };
    if should_send_discord {
        if let Err(e) = send_discord(&settings.discord.webhook_url, &title, &body, &level).await {
            errors.push(format!("Discord: {e}"));
        }
    }

    // Pushover
    let should_send_pushover = if send_all {
        settings.pushover.enabled
            && !settings.pushover.api_token.is_empty()
            && !settings.pushover.user_key.is_empty()
    } else {
        channel == "pushover"
            && !settings.pushover.api_token.is_empty()
            && !settings.pushover.user_key.is_empty()
    };
    if should_send_pushover {
        if let Err(e) = send_pushover(
            &settings.pushover.api_token,
            &settings.pushover.user_key,
            &title,
            &body,
            &level,
        )
        .await
        {
            errors.push(format!("Pushover: {e}"));
        }
    }

    Ok(crate::ExternalNotifyResult {
        ok: errors.is_empty(),
        errors,
    })
}
