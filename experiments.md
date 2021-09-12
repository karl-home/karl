# Experiments

## Baseline

```
./target/release/karl-controller --autoconfirm --dashboard --pubsub-enabled 1 --caching-enabled 0
sudo ./target/release/karl-host --port 59583 --controller-ip 127.0.0.1 --pubsub 1 --cold-cache 0 --warm-cache 0
```

## Cold Cache

```
./target/release/karl-controller --autoconfirm --dashboard --pubsub-enabled 1 --caching-enabled 1
sudo ./target/release/karl-host --port 59583 --controller-ip 127.0.0.1 --pubsub 1 --cold-cache 1 --warm-cache 0
```

## Warm Cache

```
./target/release/karl-controller --autoconfirm --dashboard --pubsub-enabled 1 --caching-enabled 1
sudo ./target/release/karl-host --port 59583 --controller-ip 127.0.0.1 --pubsub 1 --cold-cache 1 --warm-cache 1
```

## LivestreamOn (I)

```
./target/release/examples/camera --ip 127.0.0.1
```

Then spawn the `set_true` module.

## SpeechLight (III)

```
./target/release/examples/light --ip 127.0.0.1
./target/release/examples/speaker --interval 10 --ip 127.0.0.1
```

## PersonDet (IV)

```
./target/release/examples/occupancy_sensor --ip 127.0.0.1
./target/release/examples/camera --interval 20 --ip 127.0.0.1
```
