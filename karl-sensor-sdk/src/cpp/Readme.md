# Installing the C++ SDK
This section contains directions on how to install the SDK as a library on your machine so you can run example code and/or write your own Karl sensor drivers.

## Install GRPC
First, [install GRPC](https://grpc.io/docs/languages/cpp/quickstart/). Ensure you have a working GRPC installation before moving on (sucessfully building the example should be good enough).

## Build Library
1. Re-export install directory (may not be necessary if GRPC was just installed but ensure they are the same):
    ```
    export MY_INSTALL_DIR=$HOME/.local
    ```
2. Make build directory:
    ```
    cd karl/karl-sensor-sdk/src/cpp
    mkdir -p cmake/build && cd cmake/build
    ```
3. Run cmake:
    ```
    cmake -DCMAKE_PREFIX_PATH=$MY_INSTALL_DIR ../..
    ```
4. Make and install:
    ```
    sudo make install
    ```
Now you can use the Karl library in your projects! (with proper linking)
  ```
  #include <Karl.h>
  ```
