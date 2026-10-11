//! Browser entry point (built by Trunk). Natively there is nothing to run.

fn main() {
    #[cfg(target_arch = "wasm32")]
    rackwright_app::ui::start();
}
