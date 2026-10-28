package io.github.brew.handset;
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
        System.out.println("PASS: 8 storage request cases and permission lifecycle regression");
    }
}
