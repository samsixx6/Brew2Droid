package io.github.brew.handset;

import android.content.*;
import android.os.Bundle;
import android.view.*;
import android.widget.*;
import java.io.*;
import java.util.*;
import org.json.*;

/** Seletor nativo de caminhos reais, compatível com o VFS e as coleções já importadas. */
public final class FolderPickerActivity extends NativeUi {
    File directory;boolean fileMode,ready,storageRequested;ListView list;TextView path;Button select;
    final ArrayList<File> entries=new ArrayList<>();
    @Override public void onCreate(Bundle saved){super.onCreate(saved);fileMode=getIntent().getBooleanExtra("brew.file_mode",false);String start=saved==null?getIntent().getStringExtra("brew.path"):saved.getString("path");directory=new File(start==null?defaultRoot():start);if(directory.isFile())directory=directory.getParentFile();
        load(()->{build();ready=true;refresh();});}
    void build(){root.removeAllViews();root.addView(toolbar(tr(fileMode?"audio.soundfont":"native.games_folder"),true));LinearLayout shortcuts=new LinearLayout(this);
        shortcuts.addView(button("↑",()->{File up=directory.getParentFile();if(up!=null){directory=up;refresh();}}),new LinearLayout.LayoutParams(dp(48),dp(48)));
        shortcuts.addView(button(tr("native.imported"),()->{directory=new File(importedRoot());refresh();}),new LinearLayout.LayoutParams(0,dp(48),1));
        shortcuts.addView(button(tr("native.storage"),()->{if(needsStorageAccess()){storageRequested=true;storageAccess();}else{directory=sharedRoot();refresh();}}),new LinearLayout.LayoutParams(0,dp(48),1));root.addView(shortcuts);
        path=label(directory.toString(),13,muted);path.setPadding(dp(16),dp(8),dp(16),dp(8));root.addView(path);list=new ListView(this);root.addView(list,new LinearLayout.LayoutParams(-1,0,1));
        select=button(tr("native.choose_folder"),()->complete(directory));select.setVisibility(fileMode?View.GONE:View.VISIBLE);root.addView(select,new LinearLayout.LayoutParams(-1,dp(52)));
        list.setOnItemClickListener((p,v,position,id)->{File f=entries.get(position);if(f.isDirectory()){directory=f;refresh();}else complete(f);});}
    void refresh(){final File requested=directory;path.setText(requested.toString());select.setEnabled(false);work(()->{
            File[] found=requested.listFiles(f->f.isDirectory()||(fileMode&&f.getName().toLowerCase(Locale.ROOT).endsWith(".sf2")));if(found==null)throw new IOException("Cannot read folder. Use storage access or choose imported games.\n"+requested);
            Arrays.sort(found,Comparator.comparing((File f)->!f.isDirectory()).thenComparing(f->f.getName().toLowerCase(Locale.ROOT)));JSONArray paths=new JSONArray();for(File f:found)paths.put(f.toString());return new JSONObject().put("paths",paths);
        },result->{entries.clear();JSONArray paths=result.optJSONArray("paths");ArrayList<String> names=new ArrayList<>();for(int i=0;i<paths.length();i++){File f=new File(paths.optString(i));entries.add(f);names.add(f.getName()+(f.isDirectory()?"/":""));}list.setAdapter(new ArrayAdapter<>(this,android.R.layout.simple_list_item_1,names));select.setEnabled(true);});}
    void complete(File selected){setResult(RESULT_OK,new Intent().putExtra("brew.path",selected.toString()));finish();}
    @Override public void onRequestPermissionsResult(int request,String[] permissions,int[] grants){super.onRequestPermissionsResult(request,permissions,grants);if(request==5&&ready){if(!needsStorageAccess()){storageRequested=false;directory=sharedRoot();refresh();}else error(new SecurityException("Storage permission was not granted."));}}
    @Override protected void onResume(){super.onResume();if(storageRequested&&!needsStorageAccess()){storageRequested=false;directory=sharedRoot();}if(ready)refresh();}
    @Override protected void onSaveInstanceState(Bundle saved){saved.putString("path",directory.toString());super.onSaveInstanceState(saved);}
}
