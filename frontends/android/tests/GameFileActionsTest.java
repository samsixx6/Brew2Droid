package io.github.brew.handset;

import java.io.*;
import java.nio.file.*;
import java.nio.file.attribute.BasicFileAttributes;

public final class GameFileActionsTest {
    interface Action {void run() throws IOException;}
    static void check(boolean ok,String message){if(!ok)throw new AssertionError(message);}
    static void reject(Action action) throws IOException {try{action.run();}catch(IOException e){return;}throw new AssertionError("Unsafe folder operation accepted");}
    public static void main(String[] args) throws Exception {
        Path base=Files.createTempDirectory("brew-file-actions-").toRealPath();
        try{
            check(GameFileActions.renamedFolderName("Game (176x220) Retail 1.0","Game Retail","New").equals("New (176x220) 1.0"),"Rename dropped resolution/version");
            check(GameFileActions.renamedFolderName("Game (240×320) v1.0_TIM","Game","New").equals("New (240×320) v1.0_TIM"),"Rename changed metadata suffix");
            check(GameFileActions.renamedFolderName("Game (Unknown Resolution) Retail","Game Retail","New").equals("New (Unknown Resolution)"),"Rename lost unknown-resolution marker");
            Path root=Files.createDirectory(base.resolve("games")),folder=Files.createDirectory(root.resolve("Game"));
            Path inner=Files.createDirectory(folder.resolve("game")),module=inner.resolve("game.mod");
            Files.write(module,new byte[]{1,2,3});Files.write(folder.resolve("game.mif"),new byte[]{4,5});
            check(GameFileActions.checkedFolder(root,folder,module).equals(folder),"Expected selected game folder");
            reject(()->GameFileActions.checkedFolder(root,root,module));
            reject(()->GameFileActions.rename(root,folder,module,"../escape"));
            Path another=Files.createDirectory(root.resolve("Taken"));
            reject(()->GameFileActions.rename(root,folder,module,"Taken"));
            Path moved=GameFileActions.rename(root,folder,module,"Renamed");
            check(Files.exists(moved)&&!Files.exists(folder),"Rename must move the selected folder");
            check(java.util.Arrays.equals(Files.readAllBytes(moved),new byte[]{1,2,3}),"Rename changed module bytes");
            Path renamed=root.resolve("Renamed");
            Files.write(renamed.resolve("another.mod"),new byte[]{6});
            reject(()->GameFileActions.delete(root,renamed,moved));
            Files.delete(renamed.resolve("another.mod"));
            Path outside=Files.createDirectory(base.resolve("outside"));Files.write(outside.resolve("other.mod"),new byte[]{7});
            reject(()->GameFileActions.checkedFolder(root,outside,outside.resolve("other.mod")));
            Path save=Files.createDirectories(renamed.resolve("data/saves"));Files.write(save.resolve("progress.dat"),new byte[]{8});
            reject(()->GameFileActions.delete(root,renamed,moved,save));
            reject(()->GameFileActions.rename(root,renamed,moved,"Moved",save));
            check(Files.exists(save.resolve("progress.dat")),"Protected save changed");
            Files.delete(save.resolve("progress.dat"));Files.delete(save);Files.delete(renamed.resolve("data"));
            GameFileActions.delete(root,renamed,moved);
            check(!Files.exists(renamed)&&Files.exists(another)&&Files.exists(outside.resolve("other.mod")),"Delete affected unrelated folders");
            System.out.println("PASS: 15 scoped game-folder operation cases");
        }finally{
            Files.walkFileTree(base,new SimpleFileVisitor<Path>(){
                public FileVisitResult visitFile(Path p,BasicFileAttributes a)throws IOException{Files.delete(p);return FileVisitResult.CONTINUE;}
                public FileVisitResult postVisitDirectory(Path p,IOException e)throws IOException{Files.delete(p);return FileVisitResult.CONTINUE;}
            });
        }
    }
}
