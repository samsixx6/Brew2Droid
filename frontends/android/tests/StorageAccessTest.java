package io.github.brew.handset;
import java.io.File;
import java.util.ArrayList;
import java.util.Objects;
/** Ausência de READ em aparelhos antigos precisa abrir um pedido, não uma pasta negada. */
public final class StorageAccessTest {
    static void expect(int api,boolean read,boolean write,boolean all,String request){String actual=StorageAccess.permissionRequest(api,read,write,all);if(!Objects.equals(actual,request))throw new AssertionError("API "+api+": expected "+request+", got "+actual);}
    public static void main(String[] args){
        expect(26,false,false,false,"android.permission.WRITE_EXTERNAL_STORAGE");expect(28,false,false,false,"android.permission.WRITE_EXTERNAL_STORAGE");
        expect(29,true,true,false,null);expect(30,true,true,false,"android.settings.MANAGE_APP_ALL_FILES_ACCESS_PERMISSION");expect(34,false,false,true,null);
        for(int api:new int[]{26,28,29}){
            String request=StorageAccess.permissionRequest(api,true,false,false);
            if(!"android.permission.WRITE_EXTERNAL_STORAGE".equals(request))throw new AssertionError("API "+api+" must request write access for public game saves, got "+request);
        }
        StorageAccess.LibrarySession session=new StorageAccess.LibrarySession();
        if(session.observe(false)||session.allowed())throw new AssertionError("first launch must require access");
        session.observe(true);long granted=session.ticket();
        if(!session.accepts(granted,true))throw new AssertionError("granted load rejected");
        if(session.accepts(granted,false)||session.allowed())throw new AssertionError("revoked callback restored rows");
        session.observe(true);
        if(session.accepts(granted,true))throw new AssertionError("regrant accepted stale callback");
        if(!session.accepts(session.ticket(),true))throw new AssertionError("regrant cannot resume cache");
        removableStorage();
        System.out.println("PASS: storage requests, permission lifecycle and removable-volume navigation");
    }
    static void check(boolean condition,String message){if(!condition)throw new AssertionError(message);}
    static void removableStorage(){
        File internal=new File("/storage/emulated/0"),card=new File("/storage/1234-5678");
        ArrayList<StorageAccess.Location> roots=new ArrayList<>();
        StorageAccess.addLocation(roots,card,"SD card",false,"mounted");
        StorageAccess.addLocation(roots,internal,"Internal storage",true,"mounted");
        check(roots.size()==2,"Mounted microSD must be offered alongside internal storage");
        check(roots.get(0).directory.equals(internal.getAbsoluteFile()),"Primary storage must remain first");
        check(roots.get(1).directory.equals(card.getAbsoluteFile()),"Secondary storage must keep its real root");
        check(roots.get(1).label.equals("SD card"),"Use the Android volume description");
        StorageAccess.addLocation(roots,new File(card,"."),"Same card",false,"mounted");
        check(roots.size()==2,"Aliases of a storage root must not create duplicate devices");
        StorageAccess.addLocation(roots,new File("/storage/readonly"),"Read-only card",false,"mounted_ro");
        check(roots.size()==3,"A read-only card still permits loading games");
        for(String state:new String[]{"unmounted","removed","checking","bad_removal","unknown"})
            StorageAccess.addLocation(roots,new File("/storage/"+state),state,false,state);
        StorageAccess.addLocation(roots,null,"Unavailable",false,"mounted");
        check(roots.size()==3,"Unavailable devices must not be selectable");
        String app="com.samsixx.brew2droid";
        File appFiles=new File(card,"Android/data/"+app+"/files");
        check(card.equals(StorageAccess.rootFromAppFiles(appFiles,app)),"Android 8-10 must expose the card root, not its private app directory");
        check(StorageAccess.rootFromAppFiles(null,app)==null,"Unavailable legacy app directory must be skipped");
        check(StorageAccess.rootFromAppFiles(new File(card,"Android/data/other.app/files"),app)==null,"Reject a directory belonging to another application");
        check(StorageAccess.rootFromAppFiles(new File(appFiles,"nested"),app)==null,"Do not strip arbitrary directory components");
        check(StorageAccess.rootFromAppFiles(new File(card,"NotAndroid/data/"+app+"/files"),app)==null,"Require the documented Android directory structure");
        check(StorageAccess.sameDirectory(new File(card,"."),card),"Root navigation must recognize path aliases");
        check(!StorageAccess.sameDirectory(new File(card,"Games"),card),"A games subfolder must still navigate to its parent");
        check(!StorageAccess.sameDirectory(null,card),"Missing roots must not break parent navigation");
        StorageAccess.FolderNavigation navigation=new StorageAccess.FolderNavigation();
        navigation.remember(roots);
        roots.clear();
        navigation.remember(roots);
        check(navigation.parent(card)==null,"Ejecting a card must not navigate above its remembered root");
        check(card.equals(navigation.parent(new File(card,"Games"))),"A card subfolder must still navigate to the card root");
        StorageAccess.FolderNavigation restored=new StorageAccess.FolderNavigation(navigation.savedRoots(),true);
        check(restored.parent(new File(card,"."))==null,"Recreation must preserve the card boundary");
        check(!restored.permissionReady(false),"A pending request must wait for access");
        check(restored.requested,"Denied access must preserve the pending continuation");
        check(restored.permissionReady(true),"A grant after recreation must resume device selection");
        check(!restored.requested&&!restored.permissionReady(true),"Permission continuation must be consumed only once");
    }
}
