package io.github.brew.handset;

import java.io.IOException;
import java.nio.file.Path;
import java.nio.file.*;
import java.nio.file.attribute.BasicFileAttributes;

/** Operações de pacote; nenhuma operação pode atingir a coleção inteira ou os saves. */
final class GameFileActions {
    static String renamedFolderName(String folder,String oldTitle,String title){
        if(folder.startsWith(oldTitle))return title+folder.substring(oldTitle.length());
        // A resolução pode separar palavras do título; conservar só o metadado reconhecido.
        java.util.regex.Matcher size=java.util.regex.Pattern.compile("\\((?:[0-9]+[xX×][0-9]+|Unknown Resolution)\\)",java.util.regex.Pattern.CASE_INSENSITIVE).matcher(folder);
        if(!size.find())return title;
        String tail=folder.substring(size.end()).trim(),version="";
        java.util.regex.Matcher last=java.util.regex.Pattern.compile("(?:^|\\s)([vV]?[0-9]+\\.[0-9]+[A-Za-z0-9._-]*)$").matcher(tail);
        if(last.find())version=" "+last.group(1);
        return title+" "+size.group()+version;
    }
    static Path checkedFolder(Path root,Path folder,Path module,Path... protectedPaths) throws IOException {
        if(Files.isSymbolicLink(folder)||Files.isSymbolicLink(module))throw new IOException("Linked game folders cannot be changed here");
        Path collection=root.toRealPath(),selected=folder.toRealPath(),mod=module.toRealPath();
        if(selected.equals(collection)||!selected.startsWith(collection)||!mod.startsWith(selected)
            ||!Files.isDirectory(selected)||!Files.isRegularFile(mod)||!mod.getFileName().toString().toLowerCase(java.util.Locale.ROOT).endsWith(".mod"))
            throw new IOException("Choose a single game folder inside the current library");
        for(Path protectedPath:protectedPaths){
            Path absolute=protectedPath.toAbsolutePath().normalize(),existing=absolute;
            while(!Files.exists(existing)&&existing.getParent()!=null)existing=existing.getParent();
            Path keep=existing.toRealPath().resolve(existing.relativize(absolute)).normalize();
            if(selected.startsWith(keep)||keep.startsWith(selected))throw new IOException("This folder overlaps saved game data and cannot be changed here");
        }
        // Um pacote com módulos de outros jogos é ambíguo: nunca apagar a coleção por engano.
        try(java.util.stream.Stream<Path> files=Files.walk(selected)){
            java.util.Iterator<Path> iterator=files.iterator();int count=0;
            while(iterator.hasNext()){
                Path p=iterator.next();if(++count>100000)throw new IOException("Game folder is too large to change safely");
                if(!p.equals(mod)&&p.getFileName().toString().toLowerCase(java.util.Locale.ROOT).endsWith(".mod"))
                    throw new IOException("This folder contains more than one module. Organize each game in its own folder first.");
            }
        }
        return selected;
    }
    static Path rename(Path root,Path folder,Path module,String name,Path... protectedPaths) throws IOException {
        if(name==null||(name=name.trim()).isEmpty()||name.equals(".")||name.equals("..")||name.length()>180
            ||name.indexOf('/')>=0||name.indexOf('\\')>=0||name.chars().anyMatch(c->c<32||c==127))throw new IOException("Enter a valid folder name");
        Path selected=checkedFolder(root,folder,module,protectedPaths),relative=selected.relativize(module.toRealPath());
        Path target=selected.resolveSibling(name);
        if(target.equals(selected))return selected.resolve(relative);
        if(Files.exists(target,LinkOption.NOFOLLOW_LINKS))throw new IOException("A folder with that name already exists");
        Files.move(selected,target);
        return target.resolve(relative);
    }
    static void delete(Path root,Path folder,Path module,Path... protectedPaths) throws IOException {
        Path selected=checkedFolder(root,folder,module,protectedPaths);
        Files.walkFileTree(selected,new SimpleFileVisitor<Path>(){
            @Override public FileVisitResult visitFile(Path p,BasicFileAttributes attrs)throws IOException{Files.delete(p);return FileVisitResult.CONTINUE;}
            @Override public FileVisitResult postVisitDirectory(Path p,IOException error)throws IOException{if(error!=null)throw error;Files.delete(p);return FileVisitResult.CONTINUE;}
        });
    }
}
