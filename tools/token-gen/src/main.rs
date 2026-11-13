//! デザイントークンの生成の入口。

fn main() {
    if let Err(error) = warrior_token_gen::write_default_outputs() {
        eprintln!("token generation failed: {error}");
        std::process::exit(1);
    }
}
