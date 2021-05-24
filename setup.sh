#!/bin/bash
export RUST_LOG=info
export RUST_BACKTRACE=1
export KARL_PATH=$(pwd)
export KARL_MODULE_PATH=$(pwd)/modules

install_deps() {
	# Rust
	curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
	# Select 1) Proceed with installation
	source $HOME/.cargo/env
	rustup toolchain install nightly
	rustup default nightly
	rustup target add x86_64-unknown-linux-musl

	# Other deps
	sudo apt update
	sudo apt install -y npm python3-virtualenv firejail libsndfile1
}

init_submodules() {
	git submodule init
	git submodule update  # choose yes
}

init_aufs() {
	cd /tmp
	mkdir work module root
	sudo mount -t aufs -o br=work:module=ro none root
	sudo umount root
}

init_firejail() {
	sudo cp $KARL_PATH/data/karl.net /etc/firejail/karl.net
	sudo sed -i 's/restricted-network yes/restricted-network no/g' /etc/firejail/firejail.config
}

build_karl() {
	cd $KARL_PATH/karl-controller && cargo b --release
	cd $KARL_PATH/karl-host && cargo b --release
	cd $KARL_PATH/karl-sensor-sdk && cargo b --release --examples
}

build_ui() {
	cd $KARL_PATH/karl-ui
	npm install
	npm run build
}

build_modules() {
	cd $KARL_PATH/karl-module-sdk && \
		RUSTFLAGS="-C target-feature=+crt-static" cargo b --target x86_64-unknown-linux-musl --release --examples
	cd $KARL_PATH/data/person-detection && source setup.sh
	cd $KARL_PATH/data/picovoice && source setup.sh
	cd $KARL_PATH && mkdir modules
	cd $KARL_PATH/karl-common && cargo b --release
	./target/release/build_static
	./target/release/build_command_classifier
	./target/release/build_person_detection
}

install_deps
init_firejail
init_aufs
build_karl
build_ui
build_modules
