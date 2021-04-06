# Karl
Karl is a _home datacenter_ for IoT devices that follow a _pure-local_ privacy standard. Karl turns underutilized hardware (a router and computer) into resources that cheap IoT devices can use to mimic the functionality of the cloud, but on user-owned hardware.

## Setup
For local experiments, we used three Ubuntu 20.04 x86_64 nodes in [CloudLab](https://www.cloudlab.us/), like m510 in CloudLab Utah.
On prompt, select "1) Proceed with installation (default)". The second script also initializes the `KARL_PATH` with experiment data.

```
source scripts/install_dependencies.sh
source scripts/install_experiment_data.sh
```

Start one controller and any number of hosts. In our setup, we will start the controller on `node0` and a host on `node1`. Each node must have installed dependencies and experiment data.

```
RUST_LOG=hyper::server=info,debug ./target/debug/controller \
    --karl-path $HOME/.controller \
    --autoconfirm
```

```
sudo RUST_LOG=debug ./target/release/host \
    --karl-path $HOME/.karl \
    --controller-ip node0
```

In a secure deployment, set your own password and don't autoconfirm.

You can try offloading a simple hello world request: `cargo run --release --example hello`.

## Evaluation

### Security Camera

1. Local experiment.

```
cd karl-cpp-sdk
./linux.sh
$(./register-linux data/index.hbs node0 59582)
./person_detection-linux data/detect.py data/img.tmp node0 59582
```

2. Wyze experiment.

```
cd karl-cpp-sdk
./mipsel.sh
```

Reference: [EliasKotlyar/Xiaomi-Dafang-Hacks](https://github.com/EliasKotlyar/Xiaomi-Dafang-Hacks)

### Virtual Assistant

1. Local experiment.

```
cd karl-node-example
$(node register.js node0)
node stt_client.js node0
```

2. Almond experiment.

Reference: [stanford-oval/almond-server](https://github.com/stanford-oval/almond-server)

3. Cloud baseline.

```
python3 cloud/stt.py
```

## Karl Apps

Explore the web dashboard at `<CONTROLLER_IP>:8000` in a browser. The Security Camera app allows the user to browse photo files where a person was detected via `/storage`, and take a snapshot from the camera device via `/proxy`.

## Resources
* [Fog computing](https://en.wikipedia.org/wiki/Fog_computing)
* [Karl the Fog](https://en.wikipedia.org/wiki/San_Francisco_fog)
