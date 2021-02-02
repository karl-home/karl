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

copy_to_storage() {
	export LOCAL="$HOME/.karl/storage"
	mkdir -p $LOCAL/wyzecam $LOCAL/almond

	cp -r data/person-detection/* $LOCAL/wyzecam
	rm -r $LOCAL/wyzecam/PennFudanPed
	rm $LOCAL/wyzecam/setup.sh

	cp -r data/stt_node/* $LOCAL/almond
	rm $LOCAL/almond/weather.wav
	rm $LOCAL/almond/setup.sh
}

install_deps
init_submodules
build
cd data/person-detection
source setup.sh
cd ../stt_node
source setup.sh
cd ../..
copy_to_storage

build_cpp_sdk() {
	cd karl-cpp-sdk
	./generate.sh
	./linux.sh
	# $(./register-linux data/index.hbs node0 59582)
	# ./person_detection-linux data/detect.py data/img.tmp node0 59582
	cd ..
}

build_node_sdk() {
	cd karl-node-example
	source init.sh
	# $(node register.js node0)
	# node stt_client.js node0
	cd ..
}

RUST_LOG=hyper::server=info,debug ./target/debug/controller --karl-path /users/ygina/.karl_controller --autoconfirm
sudo RUST_LOG=debug ./target/release/host -b binary --karl-path /users/ygina/.karl
