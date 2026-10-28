package io.github.brew.handset;

import android.app.*;
import android.content.*;
import android.graphics.Color;
import android.graphics.Canvas;
import android.graphics.Paint;
import android.graphics.Typeface;
import android.os.*;
import android.view.*;
import android.widget.*;
import org.json.*;
import java.io.File;
import java.util.concurrent.*;
import java.util.function.Consumer;

/** Componentes Android compartilhados. O executor só trabalha enquanto os menus precisam de IO. */
abstract class NativeUi extends Activity {
    JSONObject data = new JSONObject(), settings = new JSONObject(), strings = new JSONObject();
    final ExecutorService io = Executors.newSingleThreadExecutor();
    volatile int generation;int pendingWrites; volatile boolean destroyed; LinearLayout root;
    int background=0xfffafafa,panel=Color.WHITE,text=0xff202124,muted=0xff60636b;
    final int accent=0xffff1f60,toolbarColor=0xff242424,dividerColor=0xffdedee1;
    int dp(float v) {return (int)(getResources().getDisplayMetrics().density*v+.5f);}
    String tr(String key) {return strings.optString(key,key);}
    String importedRoot() {File f=new File(getFilesDir(),"roms/handset-games");if(!f.isDirectory())f=new File(getFilesDir(),"roms");f.mkdirs();return f.toString();}
    boolean needsStorageAccess(){return StorageAccess.permissionRequest(Build.VERSION.SDK_INT,checkSelfPermission(android.Manifest.permission.READ_EXTERNAL_STORAGE)==android.content.pm.PackageManager.PERMISSION_GRANTED,checkSelfPermission(android.Manifest.permission.WRITE_EXTERNAL_STORAGE)==android.content.pm.PackageManager.PERMISSION_GRANTED,Build.VERSION.SDK_INT>=30&&Environment.isExternalStorageManager())!=null;}
    File sharedRoot() {
        File games=new File(Environment.getExternalStorageDirectory(),"brew2droid/games");
        if(!needsStorageAccess()&&!games.isDirectory())games.mkdirs();
        return games;
    }
    String defaultRoot() {return sharedRoot().toString();}
    String displayProfiles(){return new File(getFilesDir(),"handset-displays").toString();}
    String config() {return new File(getFilesDir(),"zeebx.json").toString();}
    static JSONObject unwrap(String json) throws Exception {JSONObject value=new JSONObject(json);if(!value.optBoolean("ok"))throw new Exception(value.optString("error","Operation failed"));return value.getJSONObject("data");}
    void accept(JSONObject value) throws JSONException {data=value;settings=value.getJSONObject("settings");strings=value.getJSONObject("strings");}
    void load(Runnable done) {work(()->unwrap(NativeBridge.snapshot(config(),defaultRoot())),v->{try{accept(v);done.run();}catch(Exception e){error(e);}});}
    boolean workAllowed(){return true;}
    long workTicket(){return 0;}
    boolean workCurrent(long ticket){return true;}
    void work(Callable<JSONObject> task, Consumer<JSONObject> done) {
        if(!workAllowed())return;
        final long ticket=workTicket();final int serial=++generation;
        io.execute(()->{if(destroyed||generation!=serial||!workAllowed()||!workCurrent(ticket))return;try{JSONObject value=task.call();runOnUiThread(()->{if(!destroyed&&generation==serial&&workAllowed()&&workCurrent(ticket))done.accept(value);});}
            catch(Throwable e){runOnUiThread(()->{if(!destroyed&&generation==serial&&workAllowed()&&workCurrent(ticket))error(e);});}});
    }
    // Escritas aceitas não são pedidos substituíveis: todas precisam terminar e reportar erro.
    void persist(Callable<JSONObject> task, Consumer<JSONObject> done) {
        pendingWrites++;
        io.execute(()->{JSONObject value=null;Throwable failure=null;try{value=task.call();}catch(Throwable e){failure=e;}
            final JSONObject result=value;final Throwable error=failure;
            runOnUiThread(()->{pendingWrites--;if(destroyed){if(error!=null)Toast.makeText(getApplicationContext(),error.getMessage(),Toast.LENGTH_LONG).show();return;}
                if(error!=null)error(error);else done.accept(result);writesFinished(error==null);
            });
        });
    }
    void writesFinished(boolean ok) {}
    @Override public void onCreate(Bundle saved) {
        setTheme(R.style.BrewTheme);
        super.onCreate(saved);
        getWindow().setStatusBarColor(toolbarColor);getWindow().setNavigationBarColor(background);
        getWindow().getDecorView().setSystemUiVisibility(View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR);
        if(Build.VERSION.SDK_INT>=29){getWindow().setStatusBarContrastEnforced(false);getWindow().setNavigationBarContrastEnforced(false);}
        // No edge-to-edge, o Android torna a barra transparente. Pintar o próprio inset
        // mantém os ícones brancos legíveis também nos aparelhos mais recentes.
        root=new LinearLayout(this){final Paint barPaint=new Paint();@Override protected void onDraw(Canvas c){super.onDraw(c);barPaint.setColor(toolbarColor);c.drawRect(0,0,getWidth(),getPaddingTop(),barPaint);}};
        root.setOrientation(LinearLayout.VERTICAL);root.setWillNotDraw(false);root.setBackgroundColor(background);setContentView(root);
        root.setOnApplyWindowInsetsListener((view,insets)->{
            if(Build.VERSION.SDK_INT>=30){android.graphics.Insets i=insets.getInsets(WindowInsets.Type.systemBars()|WindowInsets.Type.displayCutout());view.setPadding(i.left,i.top,i.right,i.bottom);}
            else view.setPadding(insets.getSystemWindowInsetLeft(),insets.getSystemWindowInsetTop(),insets.getSystemWindowInsetRight(),insets.getSystemWindowInsetBottom());
            return insets;
        });root.requestApplyInsets();
    }
    LinearLayout column(){LinearLayout v=new LinearLayout(this);v.setOrientation(LinearLayout.VERTICAL);return v;}
    TextView label(String value,int size,int color){TextView v=new TextView(this);v.setText(value);v.setTextSize(size);v.setTextColor(color);return v;}
    Button button(String title,Runnable click){Button b=new Button(this);b.setAllCaps(false);b.setText(title);b.setMinHeight(dp(48));b.setOnClickListener(v->click.run());return b;}
    ImageView icon(String kind,int color){ImageView v=new ImageView(this);v.setImageDrawable(new UiIcons(kind,color,dp(24)));v.setScaleType(ImageView.ScaleType.CENTER);v.setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_NO);return v;}
    ImageButton iconButton(String kind,String description,Runnable click){
        ImageButton button=new ImageButton(this);button.setImageDrawable(new UiIcons(kind,Color.WHITE,dp(24)));button.setScaleType(ImageView.ScaleType.CENTER);button.setContentDescription(description);
        android.util.TypedValue ripple=new android.util.TypedValue();getTheme().resolveAttribute(android.R.attr.selectableItemBackgroundBorderless,ripple,true);button.setBackgroundResource(ripple.resourceId);button.setOnClickListener(v->click.run());return button;
    }
    LinearLayout toolbar(String title,boolean back) {
        LinearLayout bar=new LinearLayout(this);bar.setGravity(Gravity.CENTER_VERTICAL);bar.setPadding(dp(4),0,dp(4),0);bar.setMinimumHeight(dp(56));bar.setBackgroundColor(toolbarColor);
        if(back)bar.addView(iconButton("back",tr("handset.settings.back"),this::finish),new LinearLayout.LayoutParams(dp(48),dp(56)));
        TextView heading=label(title,20,Color.WHITE);heading.setTypeface(Typeface.create("sans-serif-medium",Typeface.NORMAL));heading.setSingleLine(true);heading.setEllipsize(android.text.TextUtils.TruncateAt.END);heading.setPadding(dp(back?4:12),0,dp(4),0);bar.addView(heading,new LinearLayout.LayoutParams(0,-2,1));return bar;
    }
    void divider(LinearLayout body){View line=new View(this);line.setBackgroundColor(dividerColor);body.addView(line,new LinearLayout.LayoutParams(-1,1));}
    void section(LinearLayout body,String title){TextView h=label(title.toUpperCase(java.util.Locale.ROOT),13,accent);h.setTypeface(Typeface.create("sans-serif-medium",Typeface.NORMAL));h.setPadding(dp(16),dp(16),dp(16),dp(9));body.addView(h);divider(body);}
    void hint(LinearLayout body,String text){TextView h=label(text,14,muted);h.setPadding(dp(16),dp(4),dp(16),dp(12));body.addView(h);}
    void row(LinearLayout body,String title,String detail,Runnable click) {
        row(body,null,title,detail,click);
    }
    void row(LinearLayout body,String symbol,String title,String detail,Runnable click) {
        LinearLayout row=new LinearLayout(this);row.setGravity(Gravity.CENTER_VERTICAL);row.setPadding(dp(16),dp(12),dp(12),dp(12));row.setMinimumHeight(dp(64));row.setBackgroundColor(panel);
        if(symbol!=null)row.addView(icon(symbol,text),new LinearLayout.LayoutParams(dp(36),dp(32)));
        LinearLayout words=column();if(symbol!=null)words.setPadding(dp(12),0,dp(8),0);words.addView(label(title,16,text));
        if(detail!=null&&!detail.isEmpty()){TextView description=label(detail,13,muted);description.setPadding(0,dp(3),0,0);description.setMaxLines(3);description.setEllipsize(android.text.TextUtils.TruncateAt.END);words.addView(description);}
        row.addView(words,new LinearLayout.LayoutParams(0,-2,1));
        if(click!=null){row.addView(icon("chevron",muted),new LinearLayout.LayoutParams(dp(24),dp(32)));android.util.TypedValue ripple=new android.util.TypedValue();getTheme().resolveAttribute(android.R.attr.selectableItemBackground,ripple,true);row.setForeground(getDrawable(ripple.resourceId));row.setClickable(true);row.setFocusable(true);row.setOnClickListener(v->click.run());}
        body.addView(row,new LinearLayout.LayoutParams(-1,-2));divider(body);
    }
    void error(Throwable e){if(destroyed)return;new AlertDialog.Builder(this).setTitle(strings.optString("native.error","Could not complete action")).setMessage(e.getMessage()==null?e.toString():e.getMessage()).setPositiveButton(android.R.string.ok,null).show();}
    void openSettings(){startActivity(new Intent(this,SettingsActivity.class));}
    void openProfiles(){if(needsStorageAccess()){storageAccess();return;}startActivity(new Intent(this,ProfilesActivity.class));}
    void preferenceSwitch(LinearLayout body,String symbol,String title,boolean checked,Consumer<Boolean> changed,String tag){
        LinearLayout row=new LinearLayout(this);row.setGravity(Gravity.CENTER_VERTICAL);row.setPadding(dp(16),dp(12),dp(16),dp(12));row.setMinimumHeight(dp(64));row.setBackgroundColor(panel);row.addView(icon(symbol,text),new LinearLayout.LayoutParams(dp(36),dp(32)));
        TextView word=label(title,16,text);word.setPadding(dp(12),0,dp(8),0);row.addView(word,new LinearLayout.LayoutParams(0,-2,1));Switch toggle=new Switch(this);toggle.setTag(tag);toggle.setContentDescription(title);toggle.setChecked(checked);toggle.setOnCheckedChangeListener((button,on)->changed.accept(on));row.addView(toggle);row.setOnClickListener(v->toggle.performClick());body.addView(row);divider(body);
    }
    void storageAccess(){if(Build.VERSION.SDK_INT>=30){try{startActivity(new Intent(android.provider.Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION,android.net.Uri.parse("package:"+getPackageName())));}catch(ActivityNotFoundException e){startActivity(new Intent(android.provider.Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION));}}else{requestPermissions(new String[]{android.Manifest.permission.READ_EXTERNAL_STORAGE,android.Manifest.permission.WRITE_EXTERNAL_STORAGE},5);}}
    @Override protected void onDestroy(){destroyed=true;++generation;if(pendingWrites>0)io.shutdown();else io.shutdownNow();super.onDestroy();}
}
