package io.github.brew.handset;
/** Somente IO dos menus; não recebe pixels nem chamadas por quadro. */
final class NativeBridge {
    static { System.loadLibrary("zeebx_android"); }
    static native String snapshot(String settings, String defaultRoot);
    static native String update(String settings, String defaultRoot, String patch);
    static native String scan(String root, String artCache, String displayProfiles);
    static native String library(String root, String artCache, String displayProfiles, String publicProfiles, String privateRoot, String config, boolean refresh, boolean force);
    static native void cancelScan();
    static native String gameDisplay(String displayProfiles, String module, String patch);
    static native String gameOptions(String optionsRoot, String module, String patch);
    static native String profiles(String publicRoot,String privateRoot,String config,String module,String request);
    private NativeBridge() {}
}
