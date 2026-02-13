use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;
use tokio::sync::oneshot;

const CALLBACK_TIMEOUT_SECS: u64 = 300; // 5 minutes

/// Result from the callback: authorization code + state
pub struct CallbackResult {
    pub code: String,
    pub state: String,
}

/// HTML response shown to the user after callback
const SUCCESS_HTML: &str = r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>Login Success</title>
<style>body{font-family:system-ui;display:flex;align-items:center;justify-content:center;height:100vh;margin:0;background:#0d1117;color:#c9d1d9}
.box{text-align:center;padding:40px}.ok{color:#7ee787;font-size:2em;margin-bottom:16px}</style></head>
<body><div class="box"><div class="ok">&#10003;</div><p>認証が完了しました。このウィンドウを閉じてください。</p></div></body></html>"#;

const ERROR_HTML: &str = r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>Login Failed</title>
<style>body{font-family:system-ui;display:flex;align-items:center;justify-content:center;height:100vh;margin:0;background:#0d1117;color:#c9d1d9}
.box{text-align:center;padding:40px}.err{color:#ffa198;font-size:2em;margin-bottom:16px}</style></head>
<body><div class="box"><div class="err">&#10007;</div><p>認証に失敗しました。アプリに戻ってやり直してください。</p></div></body></html>"#;

/// Bind a local TCP listener on a free port and return (listener, port).
#[cfg(test)]
pub fn bind_callback_listener() -> std::io::Result<(TcpListener, u16)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    listener.set_nonblocking(true)?;
    Ok((listener, port))
}

/// Wait for the OAuth callback on the given listener.
/// Parses `?code=...&state=...` from the GET request.
///
/// This function is spawned on a background thread via tokio::task::spawn_blocking.
pub fn wait_for_callback(
    listener: TcpListener,
    mut cancel: oneshot::Receiver<()>,
) -> Result<CallbackResult, String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(CALLBACK_TIMEOUT_SECS);

    loop {
        if std::time::Instant::now() > deadline {
            return Err("OAuth callback timed out (5 minutes)".into());
        }

        // Check cancellation
        match cancel.try_recv() {
            Ok(()) | Err(oneshot::error::TryRecvError::Closed) => {
                return Err("OAuth login was cancelled".into());
            }
            Err(oneshot::error::TryRecvError::Empty) => {}
        }

        match listener.accept() {
            Ok((mut stream, _addr)) => {
                stream.set_nonblocking(false).ok();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .ok();

                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]);

                match parse_callback_request(&request) {
                    Some(result) => {
                        send_response(&mut stream, SUCCESS_HTML);
                        return Ok(result);
                    }
                    None => {
                        send_response(&mut stream, ERROR_HTML);
                        // Continue listening — might be a favicon request etc.
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                return Err(format!("Callback server error: {e}"));
            }
        }
    }
}

fn send_response(stream: &mut std::net::TcpStream, body: &str) {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

/// Parse the GET request line for `code` and `state` query parameters.
/// Supports both `?code=X&state=Y` (standard) and path fragments.
fn parse_callback_request(request: &str) -> Option<CallbackResult> {
    let first_line = request.lines().next()?;
    // e.g. "GET /callback?code=abc&state=xyz HTTP/1.1"
    let path = first_line.split_whitespace().nth(1)?;

    // Try query string
    if let Some(query) = path.split('?').nth(1) {
        let params = parse_query_string(query);
        if let (Some(code), Some(state)) = (params.get("code"), params.get("state")) {
            if !code.is_empty() && !state.is_empty() {
                return Some(CallbackResult {
                    code: code.clone(),
                    state: state.clone(),
                });
            }
        }
    }

    // Claude uses a special format: /callback with code#state in fragment
    // But fragments are not sent to the server, so Claude's callback page
    // uses JS to POST the fragment. We handle that in the body.
    // Check if there's a POST body with code and state
    if let Some(body_start) = request.find("\r\n\r\n") {
        let body = &request[body_start + 4..];
        if !body.is_empty() {
            let params = parse_query_string(body.trim());
            if let (Some(code), Some(state)) = (params.get("code"), params.get("state")) {
                if !code.is_empty() && !state.is_empty() {
                    return Some(CallbackResult {
                        code: code.clone(),
                        state: state.clone(),
                    });
                }
            }
        }
    }

    None
}

fn parse_query_string(query: &str) -> std::collections::HashMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next()?.to_string();
            let value = parts.next().unwrap_or("").to_string();
            Some((
                url_decode(&key),
                url_decode(&value),
            ))
        })
        .collect()
}

fn url_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.bytes();
    while let Some(b) = chars.next() {
        if b == b'%' {
            let hi = chars.next().and_then(|c| hex_val(c));
            let lo = chars.next().and_then(|c| hex_val(c));
            if let (Some(h), Some(l)) = (hi, lo) {
                result.push((h << 4 | l) as char);
            }
        } else if b == b'+' {
            result.push(' ');
        } else {
            result.push(b as char);
        }
    }
    result
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_standard_callback() {
        let req = "GET /callback?code=abc123&state=xyz789 HTTP/1.1\r\nHost: localhost\r\n\r\n";
        let result = parse_callback_request(req).unwrap();
        assert_eq!(result.code, "abc123");
        assert_eq!(result.state, "xyz789");
    }

    #[test]
    fn parse_post_body_callback() {
        let req = "POST /callback HTTP/1.1\r\nHost: localhost\r\nContent-Length: 23\r\n\r\ncode=abc123&state=xyz789";
        let result = parse_callback_request(req).unwrap();
        assert_eq!(result.code, "abc123");
        assert_eq!(result.state, "xyz789");
    }

    #[test]
    fn parse_callback_ignores_favicon() {
        let req = "GET /favicon.ico HTTP/1.1\r\nHost: localhost\r\n\r\n";
        assert!(parse_callback_request(req).is_none());
    }

    #[test]
    fn bind_picks_free_port() {
        let (listener, port) = bind_callback_listener().unwrap();
        assert!(port > 0);
        drop(listener);
    }
}
