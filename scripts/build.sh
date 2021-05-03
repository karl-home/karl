cargo b --release --bin controller --bin host
cargo b --release --example setup_experiment
cargo b --release --example sensor_camera
# cargo b --release --example setup_4c
# RUSTFLAGS="-C target-feature=+crt-static" cargo build --target x86_64-unknown-linux-musl --release --example dp
# RUSTFLAGS="-C target-feature=+crt-static" cargo build --target x86_64-unknown-linux-musl --release --example firmware_update
