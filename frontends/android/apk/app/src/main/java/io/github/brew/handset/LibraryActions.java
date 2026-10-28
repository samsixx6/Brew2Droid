package io.github.brew.handset;

import android.app.*;
import android.content.*;
import android.content.pm.*;
import android.graphics.drawable.Icon;
import android.widget.*;
import org.json.*;
import java.io.*;
import java.nio.file.*;
import java.util.*;

/** Ações explícitas de um jogo; só o usuário confirma a exclusão de uma pasta. */
final class LibraryActions {
    final MainActivity owner;
    LibraryActions(MainActivity owner){this.owner=owner;}
    Path saves(){return new File(android.os.Environment.getExternalStorageDirectory(),"brew2droid/data").toPath();}
    void show(JSONObject game){new AlertDialog.Builder(owner).setTitle(game.optString("title"))
        .setItems(new String[]{"Add shortcut","Rename","Settings","Delete"},(d,i)->{switch(i){case 0:shortcut(game);break;case 1:rename(game);break;case 2:owner.gameSettings(game);break;case 3:delete(game);break;}}).show();}
    void shortcut(JSONObject game){
        ShortcutManager shortcuts=owner.getSystemService(ShortcutManager.class);
        if(shortcuts==null||!shortcuts.isRequestPinShortcutSupported()){owner.error(new IOException("This launcher does not support pinned shortcuts"));return;}
        JSONObject selection=game.optJSONObject("resolution");String id=selection==null?"":selection.optString("module_sha256");if(id.isEmpty())id=Integer.toHexString(game.optString("path").hashCode());
        if(!shortcuts.requestPinShortcut(shortcutInfo(game,"brew-"+id),null))owner.error(new IOException("The launcher could not add this shortcut"));
    }
    ShortcutInfo shortcutInfo(JSONObject game,String id){
        Intent intent=new Intent(owner,MainActivity.class).setAction(Intent.ACTION_VIEW).putExtra("brew.shortcut_game",game.optString("path")).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK|Intent.FLAG_ACTIVITY_CLEAR_TOP);
        android.graphics.Bitmap bitmap=MainActivity.decodeArt(game.optString("art"));
        Icon icon=bitmap==null?Icon.createWithResource(owner,R.mipmap.ic_launcher):Icon.createWithBitmap(bitmap);
        return new ShortcutInfo.Builder(owner,id).setShortLabel(game.optString("title","Game")).setLongLabel(game.optString("title","Game")).setIcon(icon).setIntent(intent).build();
    }
    void updateShortcuts(JSONObject game,String oldPath){
        ShortcutManager manager=owner.getSystemService(ShortcutManager.class);if(manager==null)return;
        ArrayList<ShortcutInfo> updates=new ArrayList<>();
        for(ShortcutInfo shortcut:manager.getPinnedShortcuts()){Intent intent=shortcut.getIntent();if(intent!=null&&oldPath.equals(intent.getStringExtra("brew.shortcut_game")))updates.add(shortcutInfo(game,shortcut.getId()));}
        if(!updates.isEmpty())manager.updateShortcuts(updates);
    }
    void rename(JSONObject game){
        EditText input=new EditText(owner);input.setSingleLine(true);input.setText(game.optString("title"));input.setSelectAllOnFocus(true);input.setPadding(owner.dp(20),owner.dp(12),owner.dp(20),owner.dp(12));
        AlertDialog dialog=new AlertDialog.Builder(owner).setTitle("Rename").setView(input).setPositiveButton(android.R.string.ok,null).setNegativeButton(android.R.string.cancel,null).create();
        dialog.setOnShowListener(v->dialog.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener(b->{String name=input.getText().toString().trim();if(name.isEmpty()){input.setError("Enter a name");return;}dialog.dismiss();owner.stopScan();
            String oldPath=game.optString("path"),oldTitle=game.optString("title"),folder=game.optString("game_folder");
            owner.persist(()->{
                String path=oldPath,newFolder=folder;
                if("folder".equals(game.optString("title_source"))){
                    Path source=Paths.get(folder);String target=GameFileActions.renamedFolderName(source.getFileName().toString(),oldTitle,name);
                    Path moved=GameFileActions.rename(Paths.get(owner.activeRoot),source,Paths.get(oldPath),target,saves());path=moved.toString();newFolder=source.resolveSibling(target).toString();
                }
                return new JSONObject().put("path",path).put("folder",newFolder);
            },result->{try{String path=result.getString("path");owner.prefs.edit().remove("alias:"+oldPath).putString("alias:"+path,name).apply();game.put("path",path).put("game_folder",result.getString("folder")).put("title",name);updateShortcuts(game,oldPath);owner.filter();owner.refreshLibrary(false);}catch(Exception e){owner.error(e);}});
        }));dialog.show();
    }
    void delete(JSONObject game){
        String folder=game.optString("game_folder"),module=game.optString("path");
        new AlertDialog.Builder(owner).setTitle("Delete game?").setMessage("Delete this game folder and its contents?\n\n"+folder+"\n\nSaved progress in brew2droid/data will be kept.")
            .setNegativeButton(android.R.string.cancel,null).setPositiveButton("Delete",(d,w)->{owner.stopScan();owner.persist(()->{GameFileActions.delete(Paths.get(owner.activeRoot),Paths.get(folder),Paths.get(module),saves());return new JSONObject();},result->{owner.games.remove(game);owner.prefs.edit().remove("alias:"+module).apply();owner.filter();owner.refreshLibrary(false);});}).show();
    }
}
