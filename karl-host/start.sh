cargo b --release && sudo RUST_LOG=h2=info,hyper=info,tower=info,debug ./target/release/karl-host --port 59583
