#[expect(
    clippy::print_stderr,
    reason = "host-side stub that exists solely to remind the operator to target wasm32-wasip2"
)]
fn main() {
    eprintln!("Run `spin up` or target wasm32-wasip2 to execute mocktioneer-adapter-spin.");
}
