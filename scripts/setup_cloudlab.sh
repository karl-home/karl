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
	sudo apt install -y python3-pip virtualenv
	sudo apt install -y libavahi-compat-libdnssd-dev
}

init_submodules() {
	git submodule init
	git submodule update
}

build() {
	cargo b --release
}

install_deps
init_submodules
build
./scripts/setup_stt.sh
