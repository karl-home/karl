# karl-module-sdk

Modules analyze data from sensors and other modules. They can also change
sensor state and access the network, with the correct permissions.

Build the modules as static binaries.

```
RUSTFLAGS="-C target-feature=+crt-static" cargo build --target x86_64-unknown-linux-musl --release --examples
../karl-common/target/release/hookgen
```

Generate hooks from the static binaries.

```
./target/release/hookgen
```

| Module ID            | Input Params | Output Tags | Network |
| -------------------- | ------------ | ----------- | ------- |
| differential_privacy | count        | -           | yes     |
| false                | -            | false       | no      |
| firmware_update      | -            | firmware    | yes     |
| light_switch         | light_intent | state       | no      |
| search               | query_intent | response    | yes     |
| targz                | files        | video       | no      |
| true                 | -            | true        | no      |

The file to add is just the static binary at
`target/x86_64-unknown-linux-musl/release/examples/`.

## Generating Python modules

| Module ID          | Input Params | Output Tags         | Network |
| ------------------ | ------------ | ------------------- | ------- |
| command_classifier | sound        | light,search        | no      |
| person_detection   | image        | box,all_count,count | no      |

Running `./karl-common/target/release/hookgen`:

```
# Person detection - `env/bin/python detect.py`
data/person-detection/env env
data/person-detection/torch torch
data/person-detection/detect.py detect.py
data/person-detection/karl.py karl.py
data/person-detection/request_pb2.py request_pb2.py
data/person-detection/request_pb2_grpc.py request_pb2_grpc.py

# Command classifier - `env/bin/python picovoice_demo_file.py`
data/picovoice/env env
data/picovoice/picovoice_demo_file.py picovoice_demo_file.py
data/picovoice/picovoice_linux.ppn picovoice_linux.ppn
data/picovoice/coffee_maker_linux.rhn coffee_maker_linux.rhn
data/picovoice/libsndfile.so.1 libsndfile.so.1
data/picovoice/request_pb2_grpc.py request_pb2_grpc.py
data/picovoice/request_pb2.py request_pb2.py
data/picovoice/karl.py karl.py
```

## Installing MUSL libc

By default, Rust statically links all Rust code. However, if you use the
standard libray, it will dynamically link to the system's `libc` implementation.
In that case, we need the `MUSL libc`:

```
rustup target add x86_64-unknown-linux-musl
rustup target add x86_64-unknown-linux-musl --toolchain=nightly
```

I may have also needed to run these commands at some point:

```
sudo apt install musl-tools
ln -s /usr/include/x86_64-linux-gnu/asm /usr/include/x86_64-linux-musl/asm &&     ln -s /usr/include/asm-generic /usr/include/x86_64-linux-musl/asm-generic &&     ln -s /usr/include/linux /usr/include/x86_64-linux-musl/linux
```

OpenSSL error:

```
mkdir /musl
wget https://github.com/openssl/openssl/archive/OpenSSL_1_1_1f.tar.gz
tar zxvf OpenSSL_1_1_1f.tar.gz
cd openssl-OpenSSL_1_1_1f/
CC="musl-gcc -fPIE -pie" ./Configure no-shared no-async --prefix=/musl --openssldir=/musl/ssl linux-x86_64
make depend
make -j$(nproc)
make install
export PKG_CONFIG_ALLOW_CROSS=1
export OPENSSL_STATIC=true
export OPENSSL_DIR=/musl
```
