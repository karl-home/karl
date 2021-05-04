# picovoice

The demo code and models are adapted from the [Picovoice repository](https://github.com/Picovoice/picovoice).

```
./setup.sh
```

To ensure that picovoice can run statically:

1. In `env/lib/python3.8/site-packages/soundfile.py`, go to the line that says
`_libname = _find_library('sndfile')` and find the value of `_libname`.
In my case, that's `libsndfile.so.1`.
2. Change the line to `_libname = 'libsndfile.so.1'`.
3. Now find out where that file is in your system.
In my case, I found it in `/usr/lib/x86_64-linux-gnu/libsndfile.so.1`.
4. Copy the file to the current directory:
`cp /usr/lib/x86_64-linux-gnu/libsndfile.so.1 .`.

There is a `libsndfile.so.1` file committed in case that works by default.
But you still need to change the line in the library.
