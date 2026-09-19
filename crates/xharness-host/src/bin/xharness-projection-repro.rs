//! Opt-in offline executable: never starts the desktop/Host/Agent runtime.
#[tokio::main(flavor = "current_thread")]
async fn main() {
    // A malformed private journal must not leak its contents through panic output.
    std::panic::set_hook(Box::new(|_| {
        eprintln!("projection-repro: panic; inspect local checkpoint/dump")
    }));
    if let Err(error) =
        xharness_host::projection_repro::run_cli(std::env::args().skip(1).collect()).await
    {
        eprintln!("projection-repro: {error}");
        std::process::exit(1);
    }
}
