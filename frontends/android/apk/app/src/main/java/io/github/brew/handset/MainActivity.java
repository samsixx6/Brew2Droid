package io.github.brew.handset;

import android.app.*;
import android.content.*;
import android.graphics.*;
import android.os.Bundle;
import android.text.TextUtils;
import android.view.inputmethod.InputMethodManager;
import android.text.*;
import android.view.*;
import android.widget.*;
import org.json.*;
import java.io.*;
import java.nio.*;
import java.nio.file.Files;
import java.util.*;

/** A biblioteca deixa de redesenhar egui e de manter a emulação ativa no menu. */
public final class MainActivity extends NativeUi {
    static final int PLAY=40,SETTINGS=41,GAME_SETTINGS=42;
    AbsListView list;FrameLayout content;LinearLayout searchRow;TextView status,empty;EditText search;Adapter adapter;
    ImageButton layoutButton,sortButton,filterButton,settingsButton;boolean busy,grid,scanning,returnedFromGame,forceRescan;volatile boolean launching;int sortMode,scrollPosition,scrollOffset,artGeneration;String query="",activeRoot="",shortcutPath;
    final StorageAccess.LibrarySession storageSession=new StorageAccess.LibrarySession();boolean permissionRequired,libraryStarted;
    android.content.SharedPreferences prefs;
    final Set<String> hiddenResolutions=new HashSet<>(),hiddenCompanies=new HashSet<>();
    final java.util.concurrent.ExecutorService thumbnailIo=java.util.concurrent.Executors.newSingleThreadExecutor();
    final Set<String> loadingArt=java.util.Collections.synchronizedSet(new HashSet<>());
    final ArrayList<JSONObject> games=new ArrayList<>(),visible=new ArrayList<>();
    final android.util.LruCache<String,Bitmap> images=new android.util.LruCache<String,Bitmap>(2*1024*1024){protected int sizeOf(String key,Bitmap b){return b.getAllocationByteCount();}};
    @Override public void onCreate(Bundle saved){
        super.onCreate(saved);prefs=getSharedPreferences("library-ui",MODE_PRIVATE);
        grid=prefs.getBoolean("grid",false);sortMode=prefs.getInt("sort",0);query=prefs.getString("query","");scrollPosition=prefs.getInt("position",0);scrollOffset=prefs.getInt("offset",0);
        hiddenResolutions.addAll(prefs.getStringSet("hidden-resolutions",Collections.emptySet()));hiddenCompanies.addAll(prefs.getStringSet("hidden-companies",Collections.emptySet()));
        if(saved!=null){grid=saved.getBoolean("grid",grid);sortMode=saved.getInt("sort",sortMode);query=saved.getString("query",query);scrollPosition=saved.getInt("position",scrollPosition);scrollOffset=saved.getInt("offset",scrollOffset);}
        shortcutPath=getIntent().getStringExtra("brew.shortcut_game");build();
    }
    private void build(){
        rememberScroll();root.removeAllViews();
        if(needsStorageAccess()){showStorageRequired();return;}
        LinearLayout actions=toolbar("B2D",false);actions.setTag("brew.library.toolbar");
        actions.getChildAt(0).setLayoutParams(new LinearLayout.LayoutParams(-2,-2));root.addView(actions);
        ImageButton find=iconButton("search","Search",this::showSearch);find.setTag("brew.action.search");actions.addView(find,new LinearLayout.LayoutParams(0,dp(48),1));
        sortButton=iconButton("sort","Sort",()->sortMenu(sortButton));sortButton.setTag("brew.action.sort");actions.addView(sortButton,new LinearLayout.LayoutParams(0,dp(48),1));
        filterButton=iconButton("filter","Filter",this::filters);filterButton.setTag("brew.action.filter");actions.addView(filterButton,new LinearLayout.LayoutParams(0,dp(48),1));
        layoutButton=iconButton(grid?"list":"grid",grid?"List view":"Grid view",this::switchLayout);layoutButton.setTag("brew.action.layout");actions.addView(layoutButton,new LinearLayout.LayoutParams(0,dp(48),1));
        settingsButton=iconButton("settings","Settings",()->settingsMenu(settingsButton));settingsButton.setTag("brew.action.settings");actions.addView(settingsButton,new LinearLayout.LayoutParams(0,dp(48),1));
        for(int i=1;i<actions.getChildCount();i++)actions.getChildAt(i).setPadding(0,0,0,0);
        searchRow=new LinearLayout(this);searchRow.setGravity(Gravity.CENTER_VERTICAL);searchRow.setPadding(dp(12),dp(2),dp(12),dp(2));searchRow.setBackgroundColor(panel);
        search=new EditText(this);search.setTag("brew.library.search");search.setSingleLine(true);search.setTextColor(text);search.setHintTextColor(muted);search.setTextSize(16);search.setHint(strings.optString("native.search","Search games"));search.setContentDescription(search.getHint());search.setText(query);search.setPadding(dp(4),dp(8),dp(4),dp(8));searchRow.addView(search,new LinearLayout.LayoutParams(-1,dp(48)));searchRow.setVisibility(query.isEmpty()?View.GONE:View.VISIBLE);root.addView(searchRow);
        content=new FrameLayout(this);root.addView(content,new LinearLayout.LayoutParams(-1,0,1));adapter=new Adapter();list=null;mountList();
        divider(root);status=label("",13,muted);status.setPadding(dp(16),dp(10),dp(16),dp(10));root.addView(status);
        search.addTextChangedListener(new TextWatcher(){public void beforeTextChanged(CharSequence s,int a,int c,int f){}public void onTextChanged(CharSequence s,int a,int b,int c){query=s.toString();scrollPosition=scrollOffset=0;filter();}public void afterTextChanged(Editable e){}});filter();restoreScroll();
    }
    private void showStorageRequired(){
        permissionRequired=true;storageSession.observe(false);stopScan();scanning=false;busy=false;
        games.clear();visible.clear();artGeneration++;images.evictAll();
        if(list!=null)list.setAdapter(null);list=null;searchRow=null;status=null;empty=null;adapter=null;
        root.removeAllViews();root.addView(toolbar("B2D",false));
        LinearLayout gate=column();gate.setGravity(Gravity.CENTER);gate.setPadding(dp(24),dp(24),dp(24),dp(24));
        TextView explanation=label("Storage permission is required to access your games and save data.",16,muted);explanation.setGravity(Gravity.CENTER);gate.addView(explanation);
        Button allow=button("Allow Storage Permission",this::storageAccess);allow.setTag("brew.storage.allow");gate.addView(allow,new LinearLayout.LayoutParams(-2,-2));root.addView(gate,new LinearLayout.LayoutParams(-1,0,1));
    }
    @Override boolean workAllowed(){
        boolean allowed=!needsStorageAccess();storageSession.observe(allowed);
        if(!allowed)runOnUiThread(()->{if(!destroyed&&needsStorageAccess())showStorageRequired();});return allowed;
    }
    @Override long workTicket(){return storageSession.ticket();}
    @Override boolean workCurrent(long ticket){return storageSession.accepts(ticket,!needsStorageAccess());}
    @Override public void onWindowFocusChanged(boolean focused){super.onWindowFocusChanged(focused);if(focused&&needsStorageAccess())showStorageRequired();}
    @Override public void onRequestPermissionsResult(int request,String[] permissions,int[] results){super.onRequestPermissionsResult(request,permissions,results);if(request==5){if(needsStorageAccess())showStorageRequired();else resumeLibrary(false,false);}}
    void rememberScroll(){if(list!=null&&list.getChildCount()>0){scrollPosition=list.getFirstVisiblePosition();scrollOffset=list.getChildAt(0).getTop()-list.getPaddingTop();}}
    void restoreScroll(){if(list==null)return;final AbsListView target=list;final int position=Math.min(scrollPosition,Math.max(0,visible.size()-1)),offset=scrollOffset;target.post(()->{if(target!=list)return;target.setSelectionFromTop(position,offset);});}
    private void mountList(){
        rememberScroll();content.removeAllViews();
        if(grid){GridView v=new GridView(this);v.setNumColumns(3);v.setStretchMode(GridView.STRETCH_COLUMN_WIDTH);v.setHorizontalSpacing(dp(1));v.setVerticalSpacing(dp(1));v.setBackgroundColor(dividerColor);list=v;}
        else{ListView v=new ListView(this);v.setDivider(new android.graphics.drawable.ColorDrawable(dividerColor));v.setDividerHeight(1);v.setItemsCanFocus(false);list=v;}
        list.setDrawSelectorOnTop(false);list.setSelector(new android.graphics.drawable.ColorDrawable(0xffbdbdbd));
        list.setOnItemSelectedListener(new AdapterView.OnItemSelectedListener(){public void onItemSelected(AdapterView<?> parent,View view,int position,long id){markSelection();}public void onNothingSelected(AdapterView<?> parent){markSelection();}});
        list.setOnFocusChangeListener((view,focused)->markSelection());
        list.setTag("brew.library");list.setAdapter(adapter);list.setOnItemClickListener((p,v,pos,id)->play(visible.get(pos)));list.setOnItemLongClickListener((p,v,pos,id)->{if(!launching&&pendingWrites==0)new LibraryActions(this).show(visible.get(pos));return true;});content.addView(list,new FrameLayout.LayoutParams(-1,-1));
        empty=label("",16,muted);empty.setGravity(Gravity.CENTER);empty.setPadding(dp(28),dp(28),dp(28),dp(28));empty.setVisibility(View.GONE);content.addView(empty,new FrameLayout.LayoutParams(-1,-1));restoreScroll();
    }
    void markSelection(){if(list==null)return;int selected=list.getSelectedItemPosition();for(int i=0;i<list.getChildCount();i++){View row=list.getChildAt(i);row.setBackgroundColor(list.hasFocus()&&list.getFirstVisiblePosition()+i==selected?0xffbdbdbd:panel);}}
    private void switchLayout(){rememberScroll();grid=!grid;mountList();layoutButton.setImageDrawable(new UiIcons(grid?"list":"grid",Color.WHITE,dp(24)));layoutButton.setContentDescription(grid?"List view":"Grid view");filter();restoreScroll();}
    private void showSearch(){searchRow.setVisibility(View.VISIBLE);search.requestFocus();((InputMethodManager)getSystemService(INPUT_METHOD_SERVICE)).showSoftInput(search,InputMethodManager.SHOW_IMPLICIT);}
    private void hideSearch(){search.setText("");searchRow.setVisibility(View.GONE);((InputMethodManager)getSystemService(INPUT_METHOD_SERVICE)).hideSoftInputFromWindow(search.getWindowToken(),0);if(list!=null)list.requestFocus();}
    private void sortMenu(View anchor){PopupMenu menu=new PopupMenu(this,anchor);String[] names={"native.sort_name","native.sort_name_reverse","native.sort_resolution","native.sort_size"};for(int i=0;i<names.length;i++)menu.getMenu().add(0,i,i,tr(names[i])).setCheckable(true).setChecked(sortMode==i);menu.setOnMenuItemClickListener(item->{sortMode=item.getItemId();filter();return true;});menu.show();}
    void filters(){
        Set<String> sizes=new TreeSet<>((a,b)->{String[] x=a.split("×"),y=b.split("×");try{int c=Integer.compare(Integer.parseInt(x[0])*Integer.parseInt(x[1]),Integer.parseInt(y[0])*Integer.parseInt(y[1]));return c==0?a.compareTo(b):c;}catch(Exception e){return a.compareTo(b);}}),companies=new TreeSet<>(String.CASE_INSENSITIVE_ORDER);
        for(JSONObject game:games){sizes.add(GamePresentation.resolution(game,"Unknown"));companies.add(company(game));}
        Set<String> excludedSizes=new HashSet<>(hiddenResolutions),excludedCompanies=new HashSet<>(hiddenCompanies);
        ScrollView scroll=new ScrollView(this);LinearLayout groups=column();groups.setPadding(dp(12),0,dp(12),dp(8));scroll.addView(groups);
        filterGroup(groups,"Resolutions",sizes,excludedSizes);filterGroup(groups,"Companies",companies,excludedCompanies);
        new AlertDialog.Builder(this).setTitle("Filter games").setView(scroll).setNegativeButton("Cancel",null).setNeutralButton("Reset",(d,w)->{hiddenResolutions.clear();hiddenCompanies.clear();scrollPosition=scrollOffset=0;filter();saveUi();}).setPositiveButton("Apply",(d,w)->{hiddenResolutions.clear();hiddenResolutions.addAll(excludedSizes);hiddenCompanies.clear();hiddenCompanies.addAll(excludedCompanies);scrollPosition=scrollOffset=0;filter();saveUi();}).show();
    }
    void filterGroup(LinearLayout group,String title,Set<String> values,Set<String> excluded){
        section(group,title);LinearLayout header=new LinearLayout(this);header.setGravity(Gravity.CENTER_VERTICAL);CheckBox all=new CheckBox(this);all.setText("All "+title.toLowerCase(Locale.ROOT));all.setTextColor(text);all.setButtonTintList(android.content.res.ColorStateList.valueOf(accent));all.setChecked(Collections.disjoint(values,excluded));header.addView(all,new LinearLayout.LayoutParams(0,dp(48),1));Button clear=button("Clear",()->{});header.addView(clear);group.addView(header);
        ArrayList<CheckBox> children=new ArrayList<>();all.setOnClickListener(v->{for(CheckBox child:children){boolean checked=all.isChecked();child.setChecked(checked);if(checked)excluded.remove(child.getText().toString());else excluded.add(child.getText().toString());}});
        clear.setOnClickListener(v->{excluded.addAll(values);all.setChecked(false);for(CheckBox child:children)child.setChecked(false);});
        for(String value:values){CheckBox box=new CheckBox(this);box.setText(value);box.setTextColor(text);box.setButtonTintList(android.content.res.ColorStateList.valueOf(accent));box.setChecked(!excluded.contains(value));box.setMinHeight(dp(44));box.setOnClickListener(v->{if(box.isChecked())excluded.remove(value);else excluded.add(value);all.setChecked(Collections.disjoint(values,excluded));});children.add(box);group.addView(box,new LinearLayout.LayoutParams(-1,-2));}
    }
    static String company(JSONObject game){String value=game.isNull("company")?"":game.optString("company","");return value.isEmpty()?"Unknown":value;}
    private void settingsMenu(View anchor){
        PopupMenu menu=new PopupMenu(this,anchor,Gravity.RIGHT);
        menu.getMenu().add(0,0,0,"Settings");
        menu.getMenu().add(0,1,1,"Refresh");
        menu.setOnMenuItemClickListener(item->{if(item.getItemId()==0)openSettings();else if(item.getItemId()==1)refresh();else return false;return true;});
        menu.show();
    }
    @Override void openSettings(){stopScan();startActivityForResult(new Intent(this,SettingsActivity.class),SETTINGS);}
    void gameSettings(JSONObject game){stopScan();startActivityForResult(new Intent(this,GameSettingsActivity.class).putExtra("brew.game_path",game.optString("path")).putExtra("brew.game_title",game.optString("title")).putExtra("brew.game_art",game.optString("art")),GAME_SETTINGS);}
    @Override protected void onResume(){super.onResume();launching=false;returnedFromGame=false;
        if(needsStorageAccess()){showStorageRequired();return;}storageSession.observe(true);
        boolean check=!libraryStarted&&!permissionRequired;permissionRequired=false;libraryStarted=true;
        resumeLibrary(check,forceRescan);forceRescan=false;
    }
    String library(boolean refresh,boolean force,String folder)throws Exception{return NativeBridge.library(folder,new File(getCacheDir(),"native-art").toString(),displayProfiles(),ProfileTarget.publicRoot(),getFilesDir().toString(),config(),refresh,force);}
    void resumeLibrary(boolean check,boolean force){if(!workAllowed())return;storageSession.observe(true);load(()->{
        String folder=settings.optString("roms_dir",defaultRoot());
        File selected=new File(folder);if(!selected.isDirectory()||!selected.canRead())folder=defaultRoot();
        boolean changed=!folder.equals(activeRoot);activeRoot=folder;if(changed)games.clear();build();final String chosen=folder;
        work(()->unwrap(library(false,false,chosen)),value->{acceptGames(value);if(!launchShortcut()&&(force||check||!value.optBoolean("cached")))refreshLibrary(force);});
    });}
    void acceptGames(JSONObject value){if(!workAllowed())return;rememberScroll();games.clear();JSONArray items=value.optJSONArray("games");if(items!=null)for(int i=0;i<items.length();i++){JSONObject game=items.optJSONObject(i);if(game==null)continue;String alias=prefs.getString("alias:"+game.optString("path"),null);if(alias!=null)try{game.put("title",alias);}catch(JSONException ignored){}games.add(game);}busy=false;filter();restoreScroll();}
    void refresh(){refreshLibrary(true);}
    void refreshLibrary(boolean force){if(launching||destroyed||!workAllowed())return;scanning=true;busy=games.isEmpty();filter();String folder=activeRoot;work(()->unwrap(library(true,force,folder)),value->{scanning=false;acceptGames(value);if(value.has("refresh_error"))Toast.makeText(this,value.optString("refresh_error"),Toast.LENGTH_LONG).show();});}
    void stopScan(){++generation;NativeBridge.cancelScan();scanning=false;busy=false;}
    boolean launchShortcut(){if(shortcutPath==null||!workAllowed())return false;String path=shortcutPath;shortcutPath=null;for(JSONObject game:games)if(path.equals(game.optString("path"))){play(game);return true;}try{play(new JSONObject().put("path",path).put("title",new File(path).getName()));return true;}catch(Exception e){error(e);return false;}}
    void saveUi(){if(prefs==null)return;prefs.edit().putBoolean("grid",grid).putInt("sort",sortMode).putString("query",query).putInt("position",scrollPosition).putInt("offset",scrollOffset).putStringSet("hidden-resolutions",new HashSet<>(hiddenResolutions)).putStringSet("hidden-companies",new HashSet<>(hiddenCompanies)).apply();}
    void filter(){
        if(needsStorageAccess()){showStorageRequired();return;}visible.clear();String needle=query.trim().toLowerCase(Locale.ROOT);
        for(JSONObject game:games)if(LibraryRules.matches(GamePresentation.resolution(game,"Unknown"),company(game),hiddenResolutions,hiddenCompanies)&&(game.optString("title").toLowerCase(Locale.ROOT).contains(needle)||game.optString("path").toLowerCase(Locale.ROOT).contains(needle)))visible.add(game);
        Comparator<JSONObject> names=Comparator.comparing((JSONObject g)->g.optString("title").toLowerCase(Locale.ROOT)).thenComparing(g->g.optString("path"));Comparator<JSONObject> order=names;
        if(sortMode==1)order=names.reversed();else if(sortMode==2)order=Comparator.comparingLong(MainActivity::pixels).thenComparing(names);else if(sortMode==3)order=Comparator.comparingLong((JSONObject g)->g.optLong("folder_size_bytes",-1)).reversed().thenComparing(names);
        visible.sort(order);if(adapter!=null)adapter.notifyDataSetChanged();
        if(status!=null){String count=visible.size()==games.size()?games.size()+" game(s)":visible.size()+" of "+games.size()+" game(s)";status.setText(scanning?count+" · Checking for changes…":count);}
        if(empty!=null){empty.setVisibility(visible.isEmpty()?View.VISIBLE:View.GONE);empty.setText(games.isEmpty()?(scanning?"Scanning library…":"No games found in "+activeRoot):"No games match your search and filters");}
        saveUi();
    }
    private static long pixels(JSONObject game){JSONObject value=game.optJSONObject("resolution"),geometry=value==null?null:value.optJSONObject("geometry");return geometry==null?Long.MAX_VALUE:(long)geometry.optInt("width")*geometry.optInt("height");}
    private void play(JSONObject game){if(launching||pendingWrites>0)return;if(needsStorageAccess()){new AlertDialog.Builder(this).setTitle("Storage access").setMessage("Allow file access so Brew2Droid can save progress in Internal storage/brew2droid/data. Then open the game again.").setPositiveButton("Allow",(d,w)->storageAccess()).setNegativeButton(android.R.string.cancel,null).show();return;}File file=new File(game.optString("path"));if(!file.isFile()){error(new IOException("Game module is missing: "+file));return;}
        rememberScroll();saveUi();launching=true;stopScan();artGeneration++;images.evictAll();startActivityForResult(new Intent(this,GameActivity.class).putExtra("brew.game_path",file.toString()),PLAY);
    }
    String dimensions(JSONObject value){return value==null?"240×320":value.optInt("width",240)+"×"+value.optInt("height",320);}
    void resolution(JSONObject game){if(busy||launching||pendingWrites>0)return;
        JSONObject selection=game.optJSONObject("resolution"),automatic=selection==null?null:selection.optJSONObject("automatic");
        String auto=tr("handset.resolution.auto");
        if(automatic!=null)auto+=" · "+dimensions(automatic.optJSONObject("geometry"))+" · "+tr("handset.resolution."+automatic.optString("source","fallback"));
        String[] labels={auto,"128×160","176×208","176×220","240×320","320×240","320×480","480×800","800×480",tr("handset.resolution.custom")};
        int chosen=0;if(selection!=null&&selection.optString("source").equals("manual")){chosen=labels.length-1;String size=dimensions(selection.optJSONObject("geometry"));for(int i=1;i<labels.length-1;i++)if(labels[i].equals(size))chosen=i;}
        new AlertDialog.Builder(this).setTitle(tr("handset.resolution.title"))
            .setSingleChoiceItems(labels,chosen,(dialog,which)->{dialog.dismiss();if(which==0)saveResolution(game,"null");else if(which==labels.length-1)customResolution(game);else{String[] sides=labels[which].split("×");saveResolution(game,"{\"width\":"+sides[0]+",\"height\":"+sides[1]+",\"depth\":16}");}})
            .setNegativeButton(android.R.string.cancel,null).show();
    }
    void customResolution(JSONObject game){LinearLayout fields=column();fields.setPadding(dp(20),0,dp(20),0);JSONObject selection=game.optJSONObject("resolution"),size=selection==null?null:selection.optJSONObject("geometry");
        EditText width=new EditText(this),height=new EditText(this);EditText[] inputs={width,height};String[] keys={"width","height"};
        for(int i=0;i<inputs.length;i++){EditText input=inputs[i];input.setInputType(android.text.InputType.TYPE_CLASS_NUMBER);input.setSingleLine(true);input.setHint(tr("handset.resolution."+keys[i]));input.setContentDescription(tr("handset.resolution."+keys[i]));input.setText(Integer.toString(size==null?(i==0?240:320):size.optInt(keys[i],i==0?240:320)));fields.addView(input);}
        AlertDialog dialog=new AlertDialog.Builder(this).setTitle(tr("handset.resolution.custom")).setView(fields).setPositiveButton(android.R.string.ok,null).setNegativeButton(android.R.string.cancel,null).create();
        dialog.setOnShowListener(v->dialog.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener(b->{try{int w=Integer.parseInt(width.getText().toString()),h=Integer.parseInt(height.getText().toString());if(w<1||w>1024||h<1||h>1024)throw new NumberFormatException();saveResolution(game,new JSONObject().put("width",w).put("height",h).put("depth",16).toString());dialog.dismiss();}catch(Exception e){width.setError(tr("handset.resolution.range"));}}));dialog.show();
    }
    void saveResolution(JSONObject game,String patch){persist(()->unwrap(NativeBridge.gameDisplay(displayProfiles(),game.optString("path"),patch)),value->{try{game.put("resolution",value);game.remove("resolution_error");filter();}catch(Exception e){error(e);}});}
    @Override protected void onStop(){rememberScroll();saveUi();stopScan();artGeneration++;images.evictAll();if(list!=null)list.setAdapter(null);super.onStop();}
    @Override protected void onSaveInstanceState(Bundle saved){rememberScroll();saved.putBoolean("grid",grid);saved.putInt("sort",sortMode);saved.putString("query",query);saved.putInt("position",scrollPosition);saved.putInt("offset",scrollOffset);super.onSaveInstanceState(saved);}
    @Override protected void onNewIntent(Intent intent){super.onNewIntent(intent);setIntent(intent);shortcutPath=intent.getStringExtra("brew.shortcut_game");}
    @Override public void onBackPressed(){if(searchRow!=null&&searchRow.getVisibility()==View.VISIBLE)hideSearch();else super.onBackPressed();}
    @Override public boolean onKeyUp(int key,KeyEvent event){if(key==KeyEvent.KEYCODE_MENU){openSettings();return true;}return super.onKeyUp(key,event);}
    @Override protected void onActivityResult(int req,int result,Intent value){super.onActivityResult(req,result,value);if(req==PLAY){returnedFromGame=true;if(value!=null&&value.hasExtra("brew.error"))error(new IOException(value.getStringExtra("brew.error")));}else if(req==SETTINGS&&result==RESULT_OK)forceRescan=true;}
    @Override protected void onDestroy(){thumbnailIo.shutdownNow();super.onDestroy();}
    @Override void error(Throwable e){busy=false;super.error(e);if(status!=null)status.setText(e.getMessage());}
    final class Adapter extends BaseAdapter {
        public int getCount(){return visible.size();}public Object getItem(int p){return visible.get(p);}public long getItemId(int p){return p;}
        public View getView(int pos,View recycled,ViewGroup parent){
            JSONObject game=visible.get(pos);Holder holder=recycled!=null&&recycled.getTag() instanceof Holder?(Holder)recycled.getTag():null;if(holder==null||holder.tiles!=grid)holder=new Holder(grid);final Holder row=holder;
            row.row.setBackgroundColor(list!=null&&list.hasFocus()&&list.getSelectedItemPosition()==pos?0xffbdbdbd:panel);row.title.setText(game.optString("title"));row.detail.setText(GamePresentation.subtitle(game,tr("native.resolution_unknown"),tr("native.size_unknown")));row.version.setText(GamePresentation.version(game));row.version.setVisibility(row.version.length()==0?View.GONE:View.VISIBLE);
            row.image.setImageDrawable(new UiIcons("controls",muted,dp(32)));String art=game.isNull("art")?"":game.optString("art","");row.image.setTag(art);Bitmap cached=images.get(art);
            if(cached!=null)row.image.setImageBitmap(cached);
            else if(!art.isEmpty()&&!loadingArt.contains(art)){final int request=artGeneration;loadingArt.add(art);thumbnailIo.execute(()->{if(destroyed||launching||artGeneration!=request||needsStorageAccess()){loadingArt.remove(art);return;}Bitmap image=decodeArt(art);loadingArt.remove(art);if(image!=null)runOnUiThread(()->{if(!destroyed&&!launching&&artGeneration==request&&!needsStorageAccess()){images.put(art,image);if(art.equals(row.image.getTag()))row.image.setImageBitmap(image);}});});}
            return row.row;
        }
    }
    final class Holder {
        final LinearLayout row,words;final ImageView image;final TextView title,detail,version;final boolean tiles;
        Holder(boolean tiles){this.tiles=tiles;row=new LinearLayout(MainActivity.this);row.setTag(this);row.setOrientation(tiles?LinearLayout.VERTICAL:LinearLayout.HORIZONTAL);row.setGravity(tiles?Gravity.TOP:Gravity.CENTER_VERTICAL);row.setBackgroundColor(panel);row.setPadding(dp(tiles?6:14),dp(tiles?12:8),dp(tiles?6:12),dp(tiles?10:8));row.setMinimumHeight(dp(tiles?176:64));
            image=new ImageView(MainActivity.this);image.setScaleType(ImageView.ScaleType.FIT_CENTER);image.setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_NO);LinearLayout.LayoutParams art=new LinearLayout.LayoutParams(dp(tiles?64:44),dp(tiles?64:44));if(tiles)art.gravity=Gravity.CENTER_HORIZONTAL;row.addView(image,art);
            words=column();words.setPadding(dp(tiles?0:12),dp(tiles?10:0),0,0);title=label("",tiles?14:16,text);title.setTypeface(Typeface.DEFAULT,Typeface.BOLD);title.setMaxLines(tiles?2:1);title.setEllipsize(TextUtils.TruncateAt.END);words.addView(title);detail=label("",tiles?12:13,muted);detail.setPadding(0,dp(3),0,0);detail.setMaxLines(2);detail.setEllipsize(TextUtils.TruncateAt.END);words.addView(detail);
            row.addView(words,new LinearLayout.LayoutParams(tiles?-1:0,-2,tiles?0:1));version=label("",12,muted);version.setSingleLine(true);version.setMaxWidth(dp(76));version.setEllipsize(TextUtils.TruncateAt.END);version.setPadding(dp(tiles?0:8),dp(tiles?5:0),0,0);row.addView(version,new LinearLayout.LayoutParams(-2,-2));
        }
    }
    static Bitmap decodeArt(String path){try{File file=new File(path);if(file.length()<12||file.length()>8+192*192*4)return null;byte[] bytes=Files.readAllBytes(file.toPath());ByteBuffer b=ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN);int w=b.getInt(),h=b.getInt();if(w<1||h<1||w>192||h>192||bytes.length!=8+w*h*4)return null;int[] pixels=new int[w*h];for(int i=0;i<pixels.length;i++){int r=b.get()&255,g=b.get()&255,blue=b.get()&255,a=b.get()&255;pixels[i]=(a<<24)|(r<<16)|(g<<8)|blue;}return Bitmap.createBitmap(pixels,w,h,Bitmap.Config.ARGB_8888);}catch(Exception e){return null;}}
}
