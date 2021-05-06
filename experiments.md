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


if [ $1 -eq "0" ]; then
  echo "zero"
elif [ $1 -eq "1" ]; then
  echo "one"
elif [ $1 -eq "2" ]; then
  echo "two"
elif [ $1 -eq "3" ]; then
  echo "three"
else
  echo "0=none,1=pubsub,2=cold,3=warm"