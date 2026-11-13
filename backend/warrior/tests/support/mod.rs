//! `wrangler dev` を起動して HTTP で確かめる結合テストの支援。
//!
//! 待機は health の応答を条件にし、固定の sleep で起動を待たない。

use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(90);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// 起動した `wrangler dev`。
pub struct DevServer {
    child: Child,
    port: u16,
}

impl DevServer {
    /// `wrangler dev` をローカルモードで起動し、health が 200 になるまで待つ。
    pub fn start(port: u16) -> Self {
        let root = repository_root();
        let child = Command::new("wrangler")
            .args([
                "dev",
                "--local",
                "--ip",
                "127.0.0.1",
                "--port",
                &port.to_string(),
            ])
            .current_dir(root)
            .process_group(0)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("wrangler dev must start");
        let server = Self { child, port };
        server.wait_until_healthy();
        server
    }

    /// ヘッダを指定してリクエストを送る。
    pub fn request(&self, method: &str, path: &str, headers: &[(&str, &str)]) -> HttpResponse {
        let mut stream = self.connect(Duration::from_secs(5)).expect("connect");
        send_request(&mut stream, self.port, method, path, headers).expect("send request");
        let bytes = read_response(&mut stream);
        HttpResponse::parse(&bytes)
    }

    fn wait_until_healthy(&self) {
        let deadline = Instant::now() + STARTUP_TIMEOUT;
        loop {
            if let Ok(response) = self.try_health() {
                if response.status == 200 {
                    return;
                }
            }
            if Instant::now() >= deadline {
                panic!("the dev server did not become healthy within {STARTUP_TIMEOUT:?}");
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    }

    fn try_health(&self) -> std::io::Result<HttpResponse> {
        let mut stream = self.connect(Duration::from_millis(500))?;
        send_request(&mut stream, self.port, "GET", "/api/health", &[])?;
        Ok(HttpResponse::parse(&read_response(&mut stream)))
    }

    fn connect(&self, timeout: Duration) -> std::io::Result<TcpStream> {
        let address = SocketAddr::from(([127, 0, 0, 1], self.port));
        let stream = TcpStream::connect_timeout(&address, timeout)?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        Ok(stream)
    }
}

impl Drop for DevServer {
    fn drop(&mut self) {
        let group = -(self.child.id() as i32);
        // wrangler は workerd を別のプロセスとして起動する。プロセスグループごと止める。
        unsafe {
            libc::kill(group, libc::SIGTERM);
        }
        let _ = self.child.wait();
        unsafe {
            libc::kill(group, libc::SIGKILL);
        }
    }
}

/// HTTP の応答。
#[derive(Debug)]
pub struct HttpResponse {
    /// 状態コード。
    pub status: u16,
    /// ヘッダ。名前は小文字にする。
    pub headers: Vec<(String, String)>,
    /// 本文。
    pub body: String,
}

impl HttpResponse {
    /// ヘッダの値を小文字の名前で引く。
    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(candidate, _)| *candidate == name)
            .map(|(_, value)| value.as_str())
    }

    fn parse(bytes: &[u8]) -> Self {
        let split = header_end(bytes).expect("a response must have a header terminator");
        let header_text = String::from_utf8_lossy(&bytes[..split]);
        let mut lines = header_text.split("\r\n");
        let status_line = lines.next().expect("a status line");
        let status = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|value| value.parse().ok())
            .expect("a status code");
        let headers = lines
            .filter_map(|line| {
                let (name, value) = line.split_once(':')?;
                Some((name.trim().to_ascii_lowercase(), value.trim().to_owned()))
            })
            .collect::<Vec<(String, String)>>();
        let mut body = bytes[split + 4..].to_vec();
        if headers
            .iter()
            .any(|(name, value)| name == "transfer-encoding" && value.contains("chunked"))
        {
            body = decode_chunked(&body);
        }

        Self {
            status,
            headers,
            body: String::from_utf8_lossy(&body).into_owned(),
        }
    }
}

fn send_request(
    stream: &mut TcpStream,
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
) -> std::io::Result<()> {
    let mut request =
        format!("{method} {path} HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 0\r\n");
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes())
}

fn read_response(stream: &mut TcpStream) -> Vec<u8> {
    let mut response = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                response.extend_from_slice(&buffer[..read]);
                if response_is_complete(&response) {
                    break;
                }
            }
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                break;
            }
            Err(error) => panic!("failed to read the response: {error}"),
        }
    }
    response
}

fn response_is_complete(response: &[u8]) -> bool {
    let Some(split) = header_end(response) else {
        return false;
    };
    let header_text = String::from_utf8_lossy(&response[..split]).to_ascii_lowercase();
    if header_text.contains("transfer-encoding: chunked") {
        return chunked_complete(&response[split + 4..]);
    }
    if let Some(length) = content_length(&header_text) {
        return response.len() >= split + 4 + length;
    }
    false
}

fn header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn content_length(header_text: &str) -> Option<usize> {
    header_text.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.trim() == "content-length" {
            value.trim().parse().ok()
        } else {
            None
        }
    })
}

fn chunked_complete(body: &[u8]) -> bool {
    let mut position = 0;
    loop {
        let Some(line_end) = body[position..]
            .windows(2)
            .position(|window| window == b"\r\n")
        else {
            return false;
        };
        let size_text = String::from_utf8_lossy(&body[position..position + line_end]);
        let Ok(size) = usize::from_str_radix(size_text.trim(), 16) else {
            return false;
        };
        position += line_end + 2;
        if size == 0 {
            return body.len() >= position + 2;
        }
        position += size + 2;
        if position > body.len() {
            return false;
        }
    }
}

fn decode_chunked(body: &[u8]) -> Vec<u8> {
    let mut decoded = Vec::new();
    let mut position = 0;
    loop {
        let Some(line_end) = body[position..]
            .windows(2)
            .position(|window| window == b"\r\n")
        else {
            return decoded;
        };
        let size_text = String::from_utf8_lossy(&body[position..position + line_end]);
        let Ok(size) = usize::from_str_radix(size_text.trim(), 16) else {
            return decoded;
        };
        position += line_end + 2;
        if size == 0 {
            return decoded;
        }
        decoded.extend_from_slice(&body[position..position + size]);
        position += size + 2;
    }
}

fn repository_root() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../..")
}
