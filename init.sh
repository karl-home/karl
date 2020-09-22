#!/bin/bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
echo "export PATH=\"$HOME/.cargo/bin:$PATH\"" >> ~/.bashrc
source ~/.bashrc
sudo apt-get update
sudo apt-get install -y libavahi-compat-libdnssd-dev
mkdir ~/.wasmer
git submodule init
git submodule update
cargo b --release

