# hev-socks5-tunnel native binaries

The bundled `libhev-socks5-tunnel.so` files are built from
[`heiher/hev-socks5-tunnel`](https://github.com/heiher/hev-socks5-tunnel) commit
`180cda8b304b71b9d9ef8ea93aeb0e4e00e15f7d` with Android NDK r29.

Build flags:

```text
APP_ABI=armeabi-v7a arm64-v8a x86_64
APP_CFLAGS=-O3 -DPKGNAME=org/amnezia/awg/hevtunnel -DCLSNAME=TProxyService
```

The binaries use the boolean start/stop ABI and export native registration for
`TProxyIsRunning()`. All load segments are aligned to 16 KiB.

| ABI | SHA-256 |
| --- | --- |
| armeabi-v7a | `2e7f2739adb39a3a87485fa2166e9a7781c3f75d4fa64ac3a072ca98b725e57f` |
| arm64-v8a | `1f074e742c2f801dfd6b87bb95af8e7b80b663847104c8f53ee97ddf3651b0a4` |
| x86_64 | `b48ac92c4166a5577a301201d3606f196ebecee86c1f7f5b8aa5f1221e631626` |
