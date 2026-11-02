# Continuous integration

GitHub Actions validates the Rust workspace with `cargo fmt`, `cargo test --workspace --all-targets`, and Clippy. Separate jobs typecheck the GMX SDK adapter and typecheck/build the Next.js dashboard.
