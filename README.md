# Karl
Offload computation from your laptop, phone, and IoT devices to other devices on the local network. Move your data from the cloud to a private fog.

## Setup
Install [Rust](https://www.rust-lang.org/tools/install) using rustup. The default installation should be fine. You made need to add Cargo's bin directory (`$HOME/.cargo/bin`) to your path. I am on Cargo 1.48.0-nightly.

```
rustup toolchain install nightly
rustup default nightly
```

Install platform-specific dependencies.
* **MacOS:** None
* **Windows:** Download the [Bonjour SDK](https://developer.apple.com/bonjour/). _(WARNING: not tested)_
* **Linux:** `sudo apt-get install libavahi-compat-libdnssd-dev` or the equivalent library.

Build the binaries.

```
git submodule init
git submodule update
cargo build --release
```

## Example
The following example executes the given number of tasks across all discoverable Karl instances. Run each command in a separate terminal window.

```
cargo r --example service --release
cargo r --example parallel --release -- 20  # number of tasks
```

Prefix each command with `RUST_LOG=info` to enable logging and/or `RUST_BACKTRACE=1` to display backtraces.

## Speech-to-text

You will need to install [Virtualenv](https://virtualenv.pypa.io/en/latest/installation.html) and have Python. Initialize Python dependencies with `./scripts/setup_stt.sh`. There should be a directory at `$HOME/.karl/local/stt/` with the STT model and score, a Python binary and packages, and a file `client.py`.

### Cloud Baseline
Run the STT service. The dedicated service takes an audio file through a TCP stream (should this be an HTTP request?) where the first four bytes encode the size of the audio file. The service does STT on the audio file and returns the text to the client.

```
RUST_LOG=debug cargo r --release --example stt_standalone
RUST_LOG=debug cargo r --release --example stt_client -- --mode cloud --host <HOST_IP>
```

### Local/Karl Baseline
Offload STT to a Karl service in the same network.

```
RUST_LOG=info cargo r --release --example service -- --backend binary
RUST_LOG=debug cargo r --release --example stt_client -- --mode local --import
# sudo needed to mount filesystem :(
cargo b --release --example service && RUST_LOG=info sudo ./target/release/examples/service --backend binary
```

### Raspberry Pi Baseline
TODO

## Distributed Example

Initialize driver.
```
git clone git@github.com:ygina/karl.git
cd karl
git submodule init
git submodule update
./scripts/init.sh  # install Rust and DNS-SD library
```

Sanity check.
```
cargo b --release
RUST_LOG=karl=debug,service=debug,warn ./target/release/service
RUST_LOG=debug ./target/release/parallel -- 1
```

Distribute.
```
./scripts/sync_and_init.sh  # sync nodes in hosts.txt
./scripts/start.sh 1  # start 1 service on each node
RUST_LOG=debug ./target/release/parallel -- 1
./scripts/kill.sh  # cleanup
./scripts/retrieve_logs.sh  # get logs in ./scripts/logs/
```

## Troubleshooting

* libavahi compat error? Check platform-specific dependencies.
* Cannot resolve .local hostname? Modify [`/etc/nsswitch.conf`](https://superuser.com/questions/1417190/why-do-i-need-to-change-the-order-of-hosts-in-nsswitch-conf)
* `thread '<unnamed>' panicked at 'not implemented', /.../cranelift-codegen-0.52.0/src/isa/arm64/abi.rs:16:5`? Unfortunately, karl requires x86_64.

## Resources
* [Fog computing](https://en.wikipedia.org/wiki/Fog_computing)
* [Karl the Fog](https://en.wikipedia.org/wiki/San_Francisco_fog)
