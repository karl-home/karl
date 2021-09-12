# Installation

This document contains instructions for installing dependencies, as well as
building the `host` and `controller` binaries, the web UI, and example sensors.
The instructions assume that your current working directory is at `karl/`,
the root of this repository. We provide a script that automatically executes
the same instructions by running `source setup.sh` -- this script has been
tested on [CloudLab](https://www.cloudlab.us/) m510 nodes running Ubuntu 20.04.
However, in general we recommend you follow the instructions step-by-step
to understand how you are modifying your system.

## Build Karl

Store an absolute path to the current working directory, `karl/`, in an
environment variable.

```
export KARL_PATH=$(pwd)
```

Update packages and install [npm](https://docs.npmjs.com/downloading-and-installing-node-js-and-npm) and [firejail](https://firejail.wordpress.com/download-2/). You can check if the packages
are already installed by running `npm` and `firejail`, respectively.

```
sudo apt update && sudo apt upgrade
sudo apt install -y npm firejail
```

[Install Rust](https://www.rust-lang.org/tools/install).
When prompted, select "1) Proceed with Installation".
You can check if Rust is already installed by running `cargo`.
Install the `nightly` toolchain for the most up-to-date features,
and set it as the default.

```
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
rustup toolchain install nightly
rustup default nightly-2021-08-13
```

Setup a custom firewall such that sandboxed modules can only communicate
with the initiating host, and no one else. Configure firejail such that
non-root users can specify firewalls.

```
sudo cp $KARL_PATH/data/karl.net /etc/firejail/karl.net
sudo sed -i 's/restricted-network yes/restricted-network no/g' /etc/firejail/firejail.config
```

Initialize the [aufs](https://en.wikipedia.org/wiki/Aufs) driver, which is
used to minimize the overhead of caching module dependencies.

```
cd /tmp
mkdir work module root
sudo mount -t aufs -o br=work:module=ro none root
sudo umount root
```

Initialize the `karl-ui` git submodule. Skip this step if you cloned the
repository with the `--recurse-submodules` option.

```
cd $KARL_PATH
git submodule init
git submodule update  # choose yes
```

Build the controller and host binaries.

```
cd $KARL_PATH/karl-controller && cargo b --release
cd $KARL_PATH/karl-host && cargo b --release
```

Build the UI.

```
cd $KARL_PATH/karl-ui
npm install
npm run build
```

## Build Examples

This section assumes you have already followed the instructions to build Karl.
Your current working directory should be at `$KARL_PATH`. First, build the
example sensors, which do not require any additional dependencies.

```
cd $KARL_PATH/karl-sensor-sdk && cargo b --release --examples
```

Install Python virtualenv and a package for processing audio data.

```
sudo apt install -y python3-virtualenv libsndfile1
```

Add a Rust target for compiling static binaries, and build the Karl modules
written in Rust. Static binaries are necessary such that modules are completely
self-contained.

```
rustup target add x86_64-unknown-linux-musl
cd $KARL_PATH/karl-module-sdk
RUSTFLAGS="-C target-feature=+crt-static" cargo b --target x86_64-unknown-linux-musl --release --examples
```

Install dependencies for Karl modules written in Python. These include
a person detection and speech-to-intent module.

```
cd $KARL_PATH/data/person-detection && source setup.sh
cd $KARL_PATH/data/picovoice && source setup.sh
```

Create the target directory for compiled module bundles and store the path in
an environment variable.

```
mkdir $KARL_PATH/modules
export KARL_MODULE_PATH=$KARL_PATH/modules
```

Build and execute the binaries that compile the module bundles in the
`KARL_MODULE_PATH`.
```
cd $KARL_PATH/karl-common && cargo b --release
./target/release/build_static
./target/release/build_picovoice
./target/release/build_person_detection
```

At this point, you should be ready to run the Quick Start.
