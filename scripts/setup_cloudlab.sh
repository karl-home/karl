#!/bin/bash
install_deps() {
	# Rust
	curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
	# Select 1) Proceed with installation
	source $HOME/.cargo/env
	rustup toolchain install nightly
	rustup default nightly

	# Other deps
	sudo apt update
	sudo apt install -y protobuf-compiler python3-pip virtualenv npm

	# nvm
	curl -sL https://raw.githubusercontent.com/nvm-sh/nvm/v0.35.0/install.sh | sh
	export NVM_DIR="$HOME/.nvm"
	[ -s "$NVM_DIR/nvm.sh" ] && \. "$NVM_DIR/nvm.sh"  # This loads nvm
}

init_submodules() {
	git submodule init
	git submodule update
}

build() {
	cargo b --bin controller
	cargo b --release --bin host
}

install_deps
init_submodules
build
cd data/person-detection
source setup.sh
cd ../stt_node
source setup.sh
cd ../..


# copy data to karl cache
export LOCAL="$HOME/.karl/local/"
mkdir -p $LOCAL

cp -r data/person-detection $LOCAL
rm -r $LOCAL/person-detection/PennFudanPed
rm $LOCAL/person-detection/setup.sh

cp -r data/stt_node $LOCAL
rm $LOCAL/stt_node/weather.wav
rm $LOCAL/stt_node/setup.sh

RUST_LOG=hyper::server=info,debug ./target/debug/controller --karl-path /users/ygina/.karl_controller
sudo RUST_LOG=debug ./target/release/host -b binary --karl-path /users/ygina/.karl
