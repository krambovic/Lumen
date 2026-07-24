package org.amnezia.awg.hevtunnel;

/** Minimal JNI bridge for the bundled hev-socks5-tunnel Android library. */
public final class TProxyService {
    static {
        System.loadLibrary("hev-socks5-tunnel");
    }

    private TProxyService() {}

    public static native void TProxyStartService(String configPath, int tunFd);
    public static native void TProxyStopService();
    public static native long[] TProxyGetStats();
}
