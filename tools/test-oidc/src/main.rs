//! テスト専用の OIDC の提供者。
//!
//! Google のトークンエンドポイントと UserInfo エンドポイントと同じ形の応答を返す。
//! 本物の Google のアカウントには接続しない。本番のビルドには含めない。

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use warrior_core::base64url;

const DEFAULT_PORT: u16 = 8788;

fn main() {
    let port = parse_port();
    let issuer = format!("http://localhost:{port}");
    let listener = TcpListener::bind(("127.0.0.1", port)).expect("test OIDC provider must bind");
    let mut state = State::default();
    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            continue;
        };
        let _ = handle(stream, &mut state, &issuer);
    }
}

fn parse_port() -> u16 {
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        if argument == "--port" {
            return arguments
                .next()
                .and_then(|value| value.parse().ok())
                .unwrap_or(DEFAULT_PORT);
        }
    }
    DEFAULT_PORT
}

#[derive(Default)]
struct State {
    codes: HashMap<String, AuthorizationCode>,
    tokens: HashMap<String, User>,
    counter: u64,
}

struct AuthorizationCode {
    sub: String,
    nonce: String,
    client_id: String,
}

struct User {
    sub: String,
    email: String,
    name: String,
}

struct Request {
    method: String,
    path: String,
    query: HashMap<String, String>,
    headers: HashMap<String, String>,
    body: String,
}

fn handle(mut stream: TcpStream, state: &mut State, issuer: &str) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let Some(request) = read_request(&mut stream)? else {
        return Ok(());
    };

    let response = match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/health") => json_response(200, json!({ "status": "ok" })),
        ("GET", "/test/user") => test_user(state, &request),
        ("GET", "/authorize") => authorize(state, &request),
        ("POST", "/token") => token(state, &request, issuer),
        ("GET", "/userinfo") => userinfo(state, &request),
        _ => json_response(404, json!({ "error": "not found" })),
    };
    stream.write_all(&response)?;
    Ok(())
}

fn test_user(_state: &mut State, request: &Request) -> Vec<u8> {
    let sub = request
        .query
        .get("sub")
        .cloned()
        .unwrap_or_else(|| "test-user".to_owned());
    let cookie = format!("sub={sub}; Path=/; HttpOnly");
    response(
        200,
        "application/json",
        &[("Set-Cookie", &cookie)],
        &json!({ "sub": sub }).to_string(),
    )
}

fn authorize(state: &mut State, request: &Request) -> Vec<u8> {
    let Some(sub) = cookie_value(&request.headers, "sub") else {
        return json_response(400, json!({ "error": "missing test user" }));
    };
    let Some(redirect_uri) = request.query.get("redirect_uri") else {
        return json_response(400, json!({ "error": "missing redirect_uri" }));
    };
    let nonce = request.query.get("nonce").cloned().unwrap_or_default();
    let client_id = request.query.get("client_id").cloned().unwrap_or_default();
    state.counter += 1;
    let code = format!("code-{}", state.counter);
    state.codes.insert(
        code.clone(),
        AuthorizationCode {
            sub,
            nonce,
            client_id,
        },
    );
    let mut location = format!("{redirect_uri}?code={code}");
    if let Some(state_value) = request.query.get("state") {
        location.push_str(&format!("&state={state_value}"));
    }
    response(302, "text/plain", &[("Location", &location)], "")
}

fn token(state: &mut State, request: &Request, issuer: &str) -> Vec<u8> {
    let form = parse_query(&request.body);
    let Some(code) = form.get("code") else {
        return json_response(400, json!({ "error": "missing code" }));
    };
    let Some(authorization) = state.codes.remove(code) else {
        return json_response(400, json!({ "error": "unknown code" }));
    };

    state.counter += 1;
    let access_token = format!("token-{}", state.counter);
    let user = User {
        sub: authorization.sub.clone(),
        email: format!("{}@example.test", authorization.sub),
        name: "テスト利用者".to_owned(),
    };
    state.tokens.insert(access_token.clone(), user);

    let id_token = id_token(
        issuer,
        &authorization.sub,
        &authorization.nonce,
        &authorization.client_id,
    );
    json_response(
        200,
        json!({
            "access_token": access_token,
            "token_type": "Bearer",
            "expires_in": 3600,
            "id_token": id_token
        }),
    )
}

fn id_token(issuer: &str, sub: &str, nonce: &str, client_id: &str) -> String {
    let header = json!({ "alg": "none", "typ": "JWT" });
    let expiry = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        + 3600;
    let payload = json!({
        "iss": issuer,
        "aud": client_id,
        "sub": sub,
        "email": format!("{sub}@example.test"),
        "email_verified": true,
        "exp": expiry,
        "nonce": nonce
    });
    format!(
        "{}.{}.",
        base64url::encode(header.to_string().as_bytes()),
        base64url::encode(payload.to_string().as_bytes())
    )
}

fn userinfo(state: &State, request: &Request) -> Vec<u8> {
    let Some(header) = request.headers.get("authorization") else {
        return json_response(401, json!({ "error": "missing bearer token" }));
    };
    let Some(token) = header.strip_prefix("Bearer ") else {
        return json_response(401, json!({ "error": "invalid bearer token" }));
    };
    let Some(user) = state.tokens.get(token) else {
        return json_response(401, json!({ "error": "unknown bearer token" }));
    };
    json_response(
        200,
        json!({
            "sub": user.sub,
            "email": user.email,
            "email_verified": true,
            "name": user.name
        }),
    )
}

fn read_request(stream: &mut TcpStream) -> std::io::Result<Option<Request>> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 1024];
    let header_end = loop {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Ok(None);
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(position) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            break position;
        }
    };
    let header_text = String::from_utf8_lossy(&buffer[..header_end]);
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let target = parts.next().unwrap_or_default().to_owned();
    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    let headers = lines
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            Some((name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        })
        .collect::<HashMap<String, String>>();
    let content_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let body_start = header_end + 4;
    while buffer.len() < body_start + content_length {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    let body = String::from_utf8_lossy(&buffer[body_start..]).into_owned();
    Ok(Some(Request {
        method,
        path: path.to_owned(),
        query: parse_query(query),
        headers,
        body,
    }))
}

fn parse_query(text: &str) -> HashMap<String, String> {
    text.split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            (percent_decode(name), percent_decode(value))
        })
        .collect()
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut position = 0;
    while position < bytes.len() {
        if bytes[position] == b'%' && position + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (
                hex_value(bytes[position + 1]),
                hex_value(bytes[position + 2]),
            ) {
                decoded.push((high << 4) | low);
                position += 3;
                continue;
            }
        }
        if bytes[position] == b'+' {
            decoded.push(b' ');
        } else {
            decoded.push(bytes[position]);
        }
        position += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn cookie_value(headers: &HashMap<String, String>, name: &str) -> Option<String> {
    let cookies = headers.get("cookie")?;
    cookies.split(';').find_map(|cookie| {
        let (key, value) = cookie.trim().split_once('=')?;
        if key == name {
            Some(value.to_owned())
        } else {
            None
        }
    })
}

fn json_response(status: u16, value: Value) -> Vec<u8> {
    response(status, "application/json", &[], &value.to_string())
}

fn response(status: u16, content_type: &str, headers: &[(&str, &str)], body: &str) -> Vec<u8> {
    let reason = match status {
        200 => "OK",
        302 => "Found",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        _ => "Internal Server Error",
    };
    let mut response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (name, value) in headers {
        response.push_str(&format!("{name}: {value}\r\n"));
    }
    response.push_str("\r\n");
    response.push_str(body);
    response.into_bytes()
}
