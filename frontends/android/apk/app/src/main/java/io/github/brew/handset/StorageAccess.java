package io.github.brew.handset;
import java.io.File;
import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
/** Pedido necessário antes de navegar; separado da entrega do diálogo pelo Android. */
final class StorageAccess {
    static final class Location {
        final File directory;final String label;
        Location(File directory,String label){this.directory=directory;this.label=label;}
    }
    static File normalized(File file){
        try{return file.getCanonicalFile();}catch(IOException|SecurityException denied){return file.getAbsoluteFile();}
    }
    static boolean sameDirectory(File left,File right){return left!=null&&right!=null&&normalized(left).equals(normalized(right));}
    static void addLocation(List<Location> locations,File directory,String label,boolean primary,String state){
        if(directory==null||!("mounted".equals(state)||"mounted_ro".equals(state)))return;
        File root=normalized(directory);Location value=new Location(root,label==null||label.trim().isEmpty()?root.toString():label);
        for(int i=0;i<locations.size();i++)if(sameDirectory(locations.get(i).directory,root)){locations.set(i,value);return;}
        if(primary)locations.add(0,value);else locations.add(value);
    }
    // Antes da API 30 não existe StorageVolume.getDirectory(). Só removemos o sufixo
    // público documentado, nunca um número arbitrário de pais de um caminho recebido.
    static File rootFromAppFiles(File files,String packageName){
        if(files==null||!files.getName().equals("files"))return null;
        File app=files.getParentFile();if(app==null||!app.getName().equals(packageName))return null;
        File data=app.getParentFile();if(data==null||!data.getName().equals("data"))return null;
        File android=data.getParentFile();if(android==null||!android.getName().equals("Android"))return null;
        return android.getParentFile();
    }
    static final class FolderNavigation {
        private final ArrayList<File> roots=new ArrayList<>();boolean requested;
        FolderNavigation(){}
        FolderNavigation(List<String> saved,boolean requested){this.requested=requested;if(saved!=null)for(String root:saved)if(root!=null&&!root.isEmpty())roots.add(normalized(new File(root)));}
        void remember(List<Location> locations){
            for(Location location:locations){boolean found=false;for(File root:roots)if(sameDirectory(root,location.directory)){found=true;break;}if(!found)roots.add(location.directory);}
        }
        // Uma raiz ejetada continua sendo limite de navegação, mesmo após desaparecer
        // da lista de dispositivos disponíveis. O estado salvo conserva esse limite.
        File parent(File directory){for(File root:roots)if(sameDirectory(root,directory))return null;return directory.getParentFile();}
        ArrayList<String> savedRoots(){ArrayList<String> saved=new ArrayList<>();for(File root:roots)saved.add(root.toString());return saved;}
        boolean permissionReady(boolean granted){if(!requested||!granted)return false;requested=false;return true;}
    }
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
