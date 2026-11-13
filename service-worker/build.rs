//! Service Worker のキャッシュの版に git の commit を使う。

use std::env;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=WARRIOR_SW_VERSION");
    let version = env::var("WARRIOR_SW_VERSION")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            Command::new("git")
                .args(["rev-parse", "--short", "HEAD"])
                .output()
                .ok()
                .filter(|output| output.status.success())
                .and_then(|output| String::from_utf8(output.stdout).ok())
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| "dev".to_owned());
    println!("cargo:rustc-env=WARRIOR_SW_VERSION={version}");
    println!("cargo:rerun-if-changed=../.git/HEAD");
}
