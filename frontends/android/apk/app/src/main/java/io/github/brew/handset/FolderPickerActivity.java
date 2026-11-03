package io.github.brew.handset;

import android.content.*;
import android.os.Bundle;
import android.os.Build;
import android.os.Environment;
import android.os.storage.StorageManager;
import android.os.storage.StorageVolume;
import android.view.*;
import android.widget.*;
import java.io.*;
import java.util.*;
import org.json.*;

/** Seletor nativo de caminhos reais, compatível com o VFS e as coleções já importadas. */
public final class FolderPickerActivity extends NativeUi {
    File directory;boolean fileMode,ready;ListView list;TextView path;Button select;
    StorageAccess.FolderNavigation navigation=new StorageAccess.FolderNavigation();
    final ArrayList<File> entries=new ArrayList<>();
    @Override public void onCreate(Bundle saved){super.onCreate(saved);fileMode=getIntent().getBooleanExtra("brew.file_mode",false);String start=saved==null?getIntent().getStringExtra("brew.path"):saved.getString("path");directory=new File(start==null?defaultRoot():start);if(directory.isFile())directory=directory.getParentFile();
        if(saved!=null)navigation=new StorageAccess.FolderNavigation(saved.getStringArrayList("storage_roots"),saved.getBoolean("storage_requested",false));
        load(()->{build();ready=true;storageLocations();resumePicker();});}
    void build(){root.removeAllViews();root.addView(toolbar(tr(fileMode?"audio.soundfont":"native.games_folder"),true));LinearLayout shortcuts=new LinearLayout(this);
        shortcuts.addView(button("↑",this::parentFolder),new LinearLayout.LayoutParams(dp(48),dp(48)));
        shortcuts.addView(button(tr("native.imported"),()->{directory=new File(importedRoot());refresh();}),new LinearLayout.LayoutParams(0,dp(48),1));
        shortcuts.addView(button(tr("native.storage"),this::openStorage),new LinearLayout.LayoutParams(0,dp(48),1));root.addView(shortcuts);
        path=label(directory.toString(),13,muted);path.setPadding(dp(16),dp(8),dp(16),dp(8));root.addView(path);list=new ListView(this);root.addView(list,new LinearLayout.LayoutParams(-1,0,1));
        select=button(tr("native.choose_folder"),()->complete(directory));select.setVisibility(fileMode?View.GONE:View.VISIBLE);root.addView(select,new LinearLayout.LayoutParams(-1,dp(52)));
        list.setOnItemClickListener((p,v,position,id)->{File f=entries.get(position);if(f.isDirectory()){directory=f;refresh();}else complete(f);});}
    List<StorageAccess.Location> storageLocations(){
        ArrayList<StorageAccess.Location> locations=new ArrayList<>();
        StorageAccess.addLocation(locations,Environment.getExternalStorageDirectory(),tr("native.internal_storage"),true,Environment.getExternalStorageState());
        StorageManager manager=getSystemService(StorageManager.class);
        if(manager!=null){
            if(Build.VERSION.SDK_INT>=30){
                for(StorageVolume volume:manager.getStorageVolumes())StorageAccess.addLocation(locations,volume.getDirectory(),volume.getDescription(this),volume.isPrimary(),volume.getState());
            }else{
                for(File appFiles:getExternalFilesDirs(null))if(appFiles!=null){
                    StorageVolume volume=manager.getStorageVolume(appFiles);
                    if(volume!=null)StorageAccess.addLocation(locations,StorageAccess.rootFromAppFiles(appFiles,getPackageName()),volume.getDescription(this),volume.isPrimary(),volume.getState());
                }
            }
        }
        navigation.remember(locations);return locations;
    }
    void openStorage(){if(needsStorageAccess()){navigation.requested=true;storageAccess();}else showStorageLocations();}
    void showStorageLocations(){try{
        List<StorageAccess.Location> locations=storageLocations();if(locations.isEmpty())throw new IOException("No mounted storage is available.");
        String[] names=new String[locations.size()];for(int i=0;i<names.length;i++)names[i]=locations.get(i).label+"\n"+locations.get(i).directory;
        new android.app.AlertDialog.Builder(this).setTitle(tr("native.storage")).setItems(names,(dialog,which)->{directory=locations.get(which).directory;refresh();}).setNegativeButton(android.R.string.cancel,null).show();
    }catch(Exception e){error(e);}}
    void parentFolder(){try{
        storageLocations();File up=navigation.parent(directory);if(up==null)openStorage();else{directory=up;refresh();}
    }catch(Exception e){error(e);}}
    void refresh(){final File requested=directory;path.setText(requested.toString());select.setEnabled(false);entries.clear();list.setAdapter(new ArrayAdapter<String>(this,android.R.layout.simple_list_item_1,new ArrayList<>()));work(()->{
            File[] found=requested.listFiles(f->f.isDirectory()||(fileMode&&f.getName().toLowerCase(Locale.ROOT).endsWith(".sf2")));if(found==null)throw new IOException("Cannot read folder. Use storage access or choose imported games.\n"+requested);
            Arrays.sort(found,Comparator.comparing((File f)->!f.isDirectory()).thenComparing(f->f.getName().toLowerCase(Locale.ROOT)));JSONArray paths=new JSONArray();for(File f:found)paths.put(f.toString());return new JSONObject().put("paths",paths);
        },result->{entries.clear();JSONArray paths=result.optJSONArray("paths");ArrayList<String> names=new ArrayList<>();for(int i=0;i<paths.length();i++){File f=new File(paths.optString(i));entries.add(f);names.add(f.getName()+(f.isDirectory()?"/":""));}list.setAdapter(new ArrayAdapter<>(this,android.R.layout.simple_list_item_1,names));select.setEnabled(true);});}
    void complete(File selected){setResult(RESULT_OK,new Intent().putExtra("brew.path",selected.toString()));finish();}
    void resumePicker(){if(navigation.permissionReady(!needsStorageAccess()))showStorageLocations();refresh();}
    @Override public void onRequestPermissionsResult(int request,String[] permissions,int[] grants){super.onRequestPermissionsResult(request,permissions,grants);if(request==5&&ready){if(!needsStorageAccess()){if(navigation.permissionReady(true))showStorageLocations();}else{navigation.requested=false;error(new SecurityException("Storage permission was not granted."));}}}
    @Override protected void onResume(){super.onResume();if(ready)resumePicker();}
    @Override protected void onSaveInstanceState(Bundle saved){saved.putString("path",directory.toString());saved.putBoolean("storage_requested",navigation.requested);saved.putStringArrayList("storage_roots",navigation.savedRoots());super.onSaveInstanceState(saved);}
}
