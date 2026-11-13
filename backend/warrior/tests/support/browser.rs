//! ChromeDriver と SafariDriver の起動、および WebDriver のセッションの作成。

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use thirtyfour::prelude::*;

const DRIVER_READY_TIMEOUT: Duration = Duration::from_secs(60);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// E2E で使うブラウザ。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Browser {
    /// Chrome。
    Chrome,
    /// Safari。
    Safari,
}

impl Browser {
    /// 環境変数 `WARRIOR_E2E_BROWSER` から選ぶ。既定は Chrome。
    pub fn from_env() -> Self {
        match std::env::var("WARRIOR_E2E_BROWSER").as_deref() {
            Ok("safari") => Browser::Safari,
            _ => Browser::Chrome,
        }
    }

    fn driver_name(self) -> &'static str {
        match self {
            Browser::Chrome => "chromedriver",
            Browser::Safari => "safaridriver",
        }
    }
}

/// 起動した WebDriver。
pub struct DriverProcess {
    child: Child,
    port: u16,
}

impl DriverProcess {
    /// WebDriver を起動し、待ち受けを始めるまで待つ。
    pub fn start(browser: Browser, port: u16) -> Self {
        let mut command = Command::new(browser.driver_name());
        match browser {
            Browser::Chrome => {
                command.args([
                    format!("--port={port}"),
                    "--allowed-ips=127.0.0.1".to_owned(),
                ]);
            }
            Browser::Safari => {
                command.args(["--port", &port.to_string()]);
            }
        }
        command
            .process_group(0)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let child = command.spawn().unwrap_or_else(|error| {
            panic!(
                "{} must start (run this test with the matching mise task): {error}",
                browser.driver_name()
            )
        });
        let process = Self { child, port };
        wait_for_driver(process.port);
        process
    }

    /// WebDriver の接続先。
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

impl Drop for DriverProcess {
    fn drop(&mut self) {
        let group = -(self.child.id() as i32);
        unsafe {
            libc::kill(group, libc::SIGTERM);
        }
        let _ = self.child.wait();
        unsafe {
            libc::kill(group, libc::SIGKILL);
        }
    }
}

/// WebDriver のセッションを開く。
pub async fn open(browser: Browser, driver: &DriverProcess) -> WebDriver {
    let capabilities: thirtyfour::Capabilities = match browser {
        Browser::Chrome => {
            let mut capabilities = DesiredCapabilities::chrome();
            for argument in [
                "--headless=new",
                "--no-sandbox",
                "--disable-dev-shm-usage",
                "--disable-gpu",
                "--window-size=390,844",
                "--remote-allow-origins=*",
            ] {
                capabilities.add_arg(argument).unwrap_or_else(|error| {
                    panic!("the chrome argument {argument} must be set: {error}")
                });
            }
            capabilities.into()
        }
        Browser::Safari => DesiredCapabilities::safari().into(),
    };

    let session = WebDriver::new(driver.url(), capabilities)
        .await
        .unwrap_or_else(|error| panic!("the WebDriver session must open: {error}"));
    if browser == Browser::Chrome {
        session
            .set_window_rect(0, 0, 390, 844)
            .await
            .unwrap_or_else(|error| panic!("the window must be sized: {error}"));
    }
    session
}

/// OS に空きポートを選ばせる。
pub fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port must be available");
    listener.local_addr().expect("a local address").port()
}

fn wait_for_driver(port: u16) {
    let deadline = Instant::now() + DRIVER_READY_TIMEOUT;
    loop {
        if driver_is_ready(port) {
            return;
        }
        if Instant::now() >= deadline {
            panic!("the WebDriver on port {port} did not become ready");
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

fn driver_is_ready(port: u16) -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_millis(500)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let request =
        format!("GET /status HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut response = Vec::new();
    let mut buffer = [0u8; 1024];
    while let Ok(read) = stream.read(&mut buffer) {
        if read == 0 {
            break;
        }
        response.extend_from_slice(&buffer[..read]);
        if response.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    let text = String::from_utf8_lossy(&response);
    text.starts_with("HTTP/1.1 200") || text.starts_with("HTTP/1.0 200")
}
