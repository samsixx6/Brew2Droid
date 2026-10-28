package io.github.brew.handset;
/** Pedido necessário antes de navegar; separado da entrega do diálogo pelo Android. */
final class StorageAccess {
    static final class LibrarySession {
        private boolean granted;private long epoch;
        synchronized boolean observe(boolean access){if(granted!=access){granted=access;epoch++;return true;}return false;}
        synchronized boolean allowed(){return granted;}
        synchronized long ticket(){return epoch;}
        synchronized boolean accepts(long ticket,boolean currentAccess){observe(currentAccess);return granted&&ticket==epoch;}
    }
    static String permissionRequest(int api, boolean readGranted, boolean writeGranted, boolean allFilesGranted) {
        if(api>=30)return allFilesGranted?null:"android.settings.MANAGE_APP_ALL_FILES_ACCESS_PERMISSION";
        return readGranted&&writeGranted?null:"android.permission.WRITE_EXTERNAL_STORAGE";
    }
}
