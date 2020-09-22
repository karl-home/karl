#!/bin/bash
echo "step 1: install Rust"
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs > /tmp/rust.sh && sh /tmp/rust.sh -y && rm /tmp/rust.sh
echo "step 2: activate Rust path"
echo "export PATH=\"$HOME/.cargo/bin:$PATH\"" >> ~/.bashrc && source ~/.bashrc
echo "step 3: sudo apt-get update"
sudo apt-get update
echo "step 4: install libdnssd compat library"
sudo apt-get install -y libavahi-compat-libdnssd-dev
echo "step 5: make ~/.wasmer directory"
mkdir -p ~/.wasmer
echo "done!"
