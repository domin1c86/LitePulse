/// Probe mode writes its report to stdout. Run it through
/// `cargo run --features console -- --probe 5` so the output is visible.
pub fn run_probe(seconds: u64) {
    crate::sampler::run_probe(seconds);
}
