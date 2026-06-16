#[expect(
    clippy::print_stderr,
    reason = "host-side stub that exists solely to remind the operator to target wasm32"
)]
fn main() {
    eprintln!(
        "Run `wrangler dev` or target wasm32-unknown-unknown to execute mocktioneer-adapter-cloudflare."
    );
}
