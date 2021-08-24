# Running the Example
This section describes how to run the example "camera". Ensure you have [installed the library](/karl-sensor-sdk/src/cpp/) first. You should also start the controller before running the example executable.

1. Export local installation directory (default shown here, use whichever you installed GRPC and Karl library to):
   ```
   export MY_INSTALL_DIR=$HOME/.local
   ```
3. Run cmake
   ```
   cmake -DCMAKE_PREFIX_PATH=$MY_INSTALL_DIR ../..
   ```
4. Make example
   ```
   make -j
   ```
5. Run example
   ```
   ./example
   ```
