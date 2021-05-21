#!/bin/bash
export KARL_PATH=$(pwd)
export KARL_MODULE_PATH=$(pwd)/modules

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

build_karl() {
	cargo b --bin controller
	cargo b --release --bin host
	cargo b --release --example hello_world
}

build_cpp_sdk() {
	cd karl-cpp-sdk
	./generate.sh
	./linux.sh
	cd ..
}

build_node_sdk() {
	cd karl-node-example
	source init.sh
	cd ..
}

install_deps
init_submodules
build_karl
build_cpp_sdk
build_node_sdk
