# Experiments

## No Optimizations

Controller:
`RUST_LOG=info,karl=debug ./target/release/karl-controller --autoconfirm --pubsub 0 --caching-enabled 0`

Host:
`sudo RUST_LOG=h2=info,hyper=info,tower=info,debug ./target/release/karl-host --port 59583 --pubsub 0 --cold-cache 0 --warm-cache 0`

## PubSub

Controller:
`RUST_LOG=info,karl=debug ./target/release/karl-controller --autoconfirm --pubsub 1 --caching-enabled 0`

Host:
`sudo RUST_LOG=h2=info,hyper=info,tower=info,debug ./target/release/karl-host --port 59583 --pubsub 1 --cold-cache 0 --warm-cache 0`

## Cold Cache

Controller:
`RUST_LOG=info,karl=debug ./target/release/karl-controller --autoconfirm --pubsub 1 --caching-enabled 1`

Host:
`sudo RUST_LOG=h2=info,hyper=info,tower=info,debug ./target/release/karl-host --port 59583 --pubsub 1 --cold-cache 1 --warm-cache 0`

## Warm Cache

Controller:
`RUST_LOG=info,karl=debug ./target/release/karl-controller --autoconfirm --pubsub 1 --caching-enabled 1`

Host:
`sudo RUST_LOG=h2=info,hyper=info,tower=info,debug ./target/release/karl-host --port 59583 --pubsub 1 --cold-cache 1 --warm-cache 1`

