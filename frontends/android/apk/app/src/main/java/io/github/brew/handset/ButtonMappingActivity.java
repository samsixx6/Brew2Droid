package io.github.brew.handset;

import android.app.AlertDialog;
import android.hardware.input.InputManager;
import android.os.*;
import android.view.*;
import android.widget.*;
import org.json.JSONObject;
import static io.github.brew.handset.ButtonBindings.Target;

/** A captura continua igual; somente o alvo persistido passa a ser perfil ou jogo Custom. */
public final class ButtonMappingActivity extends NativeUi implements InputManager.InputDeviceListener {
    private ButtonMappingStore store;
    private AndroidMappingInput input;
    private ButtonMappingView keypad;
    private TextView current;
    private final java.util.EnumMap<Target,TextView> zeeboRows=new java.util.EnumMap<>(Target.class);
    private Target selected;
    private CaptureDialog capture;
    private InputManager manager;
    private boolean closeAfterWrites;
    private String word(String key,String fallback){return strings.optString(key,fallback);}
    @Override public void onCreate(Bundle state){
        super.onCreate(state);
        try{String translated=getIntent().getStringExtra("brew.strings");if(translated!=null)strings=new JSONObject(translated);}catch(Exception ignored){}
        root.addView(toolbar(word("native.button_mapping","Button Mapping"),true));
        work(()->{store=new ButtonMappingStore(this);return new JSONObject();},ignored->build());
    }
    private void build(){
        root.removeAllViews();
        input=new AndroidMappingInput((device,token,down,fresh)->{if(down&&fresh)captured(token);});
        input.setVolumeKeysMappable(store.volumeKeysMappable());
        LinearLayout bar=toolbar(word("native.button_mapping","Button Mapping"),true);
        Button reset=button(word("native.reset","Reset"),this::reset);reset.setTag("brew.mapping.reset");reset.setTextColor(android.graphics.Color.WHITE);reset.setBackgroundColor(android.graphics.Color.TRANSPARENT);bar.addView(reset,new LinearLayout.LayoutParams(dp(80),dp(56)));root.addView(bar);
        hint(root,word("native.mapping_hint","Tap a target, then press a keyboard or controller input. Assignments save automatically."));
        current=label("Mappings for "+store.profileName(),14,text);current.setTag("brew.mapping.current");current.setPadding(dp(16),0,dp(16),dp(12));current.setAccessibilityLiveRegion(View.ACCESSIBILITY_LIVE_REGION_POLITE);root.addView(current);
        if(store.isZeebo()){
            ScrollView scroll=new ScrollView(this);scroll.setFillViewport(false);
            LinearLayout list=column();zeeboRows.clear();
            for(Target target:ButtonBindings.targets(true)){
                LinearLayout row=new LinearLayout(this);row.setGravity(Gravity.CENTER_VERTICAL);row.setPadding(dp(16),dp(8),dp(16),dp(8));row.setMinimumHeight(dp(72));row.setFocusable(true);row.setTag("brew.mapping.target."+target.id);
                FrameLayout art=new FrameLayout(this);ImageView face=new ImageView(this);face.setScaleType(ImageView.ScaleType.FIT_CENTER);
                String file=target.id.startsWith("b")?"Action_"+target.id.substring(1):target==Target.ZL?"ZL":target==Target.ZR?"ZR":target==Target.HOME?"HOME":target==Target.UP||target==Target.DOWN||target==Target.LEFT||target==Target.RIGHT?"Dpad_Up":"Analog_Outer";
                try(java.io.InputStream stream=getAssets().open("zeebo/"+file+".png")){face.setImageDrawable(android.graphics.drawable.Drawable.createFromStream(stream,null));}catch(java.io.IOException e){error(e);}
                if(target==Target.RIGHT)face.setRotation(90);if(target==Target.DOWN)face.setRotation(180);if(target==Target.LEFT)face.setRotation(270);
                art.addView(face,new FrameLayout.LayoutParams(-1,-1));
                if(file.equals("Analog_Outer")){ImageView inner=new ImageView(this);inner.setScaleType(ImageView.ScaleType.FIT_CENTER);try(java.io.InputStream stream=getAssets().open("zeebo/Analog_Inner.png")){inner.setImageDrawable(android.graphics.drawable.Drawable.createFromStream(stream,null));}catch(java.io.IOException e){error(e);}FrameLayout.LayoutParams thumb=new FrameLayout.LayoutParams(dp(28),dp(28),Gravity.CENTER);art.addView(inner,thumb);}
                row.addView(art,new LinearLayout.LayoutParams(dp(76),dp(52)));TextView label=label("",16,text);label.setPadding(dp(16),0,0,0);zeeboRows.put(target,label);row.addView(label,new LinearLayout.LayoutParams(0,-2,1));row.setOnClickListener(v->select(target));list.addView(row);divider(list);
            }scroll.addView(list,new ScrollView.LayoutParams(-1,-2));refreshMapping();
            root.addView(scroll,new LinearLayout.LayoutParams(-1,0,1));
        }else{keypad=new ButtonMappingView(this,this::select);keypad.selection(null,store.current());root.addView(keypad,new LinearLayout.LayoutParams(-1,0,1));}
        getWindow().setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_STATE_ALWAYS_HIDDEN);
        setVolumeControlStream(android.media.AudioManager.STREAM_MUSIC);
        manager=(InputManager)getSystemService(INPUT_SERVICE);manager.registerInputDeviceListener(this,new Handler(Looper.getMainLooper()));
    }
    private void select(Target target){
        if(pendingWrites>0)return;
        if(capture!=null)capture.dismiss();selected=target;refreshMapping();showCurrent();
        input.beginCapture();capture=new CaptureDialog();capture.setTitle(word("native.mapping_assign","Assign input")+" · "+target.label);
        capture.setMessage(word("native.mapping_press","Press a keyboard key, controller button, D-pad or stick direction. Release a held control first.")+"\n\n"+AndroidMappingInput.label(store.current().custom(target)));
        capture.setButton(AlertDialog.BUTTON_NEGATIVE,getString(android.R.string.cancel),(dialog,which)->dialog.dismiss());
        capture.setOnDismissListener(dialog->{if(capture==dialog)capture=null;input.beginCapture();});capture.show();
    }
    private void refreshMapping(){
        if(keypad!=null)keypad.selection(selected,store.current());
        for(java.util.Map.Entry<Target,TextView> entry:zeeboRows.entrySet()){Target target=entry.getKey();TextView label=entry.getValue();label.setText(target.label+"\n"+AndroidMappingInput.label(store.current().custom(target)));label.setTextColor(target==selected?accent:text);((View)label.getParent()).setContentDescription(label.getText());}
    }
    private void showCurrent(){if(selected!=null)current.setText(selected.label+": "+AndroidMappingInput.label(store.current().custom(selected)));}
    private void captured(String token){
        if(pendingWrites>0||capture==null||!capture.isShowing()||selected==null||!ButtonBindings.validToken(token))return;
        ButtonBindings next=store.current().assign(selected,token);capture.dismiss();
        save(next,()->{refreshMapping();showCurrent();});
    }
    private void save(ButtonBindings next,Runnable done){
        persist(()->{if(!store.save(next))throw new IllegalStateException(word("native.mapping_save_failed","Could not save assignment. Previous mappings kept."));return new JSONObject();},ignored->{setResult(RESULT_OK);done.run();});
    }
    private void reset(){
        if(store==null||pendingWrites>0)return;
        new AlertDialog.Builder(this).setTitle(word("native.mapping_reset_title","Reset button mappings?"))
            .setMessage(word("native.mapping_reset_message","Restore the default keyboard and controller inputs?"))
            .setNegativeButton(android.R.string.cancel,null).setPositiveButton(word("native.reset","Reset"),(dialog,which)->{
                if(pendingWrites>0)return;
                save(ButtonBindings.defaults(store.isZeebo()),()->{selected=null;refreshMapping();current.setText(word("native.mapping_restored","Default mappings restored"));input.beginCapture();});
            }).show();
    }
    private final class CaptureDialog extends AlertDialog {
        CaptureDialog(){super(ButtonMappingActivity.this);}
        @Override public boolean dispatchKeyEvent(KeyEvent event){if(input.key(event,true))return true;return super.dispatchKeyEvent(event);}
        @Override public boolean dispatchGenericMotionEvent(MotionEvent event){return input.motion(event)||super.dispatchGenericMotionEvent(event);}
        @Override public void onWindowFocusChanged(boolean focused){super.onWindowFocusChanged(focused);if(!focused)input.beginCapture();}
    }
    @Override public boolean dispatchKeyEvent(KeyEvent event){if(input!=null&&input.key(event)&&AndroidMappingInput.volume(event))return true;return super.dispatchKeyEvent(event);}
    @Override public boolean onGenericMotionEvent(MotionEvent event){return input!=null&&input.motion(event)||super.onGenericMotionEvent(event);}
    @Override protected void onPause(){if(capture!=null)capture.dismiss();if(input!=null){input.releaseAll();input.beginCapture();}super.onPause();}
    public void onInputDeviceAdded(int id){}
    public void onInputDeviceChanged(int id){onInputDeviceRemoved(id);}
    public void onInputDeviceRemoved(int id){if(input!=null)input.releaseDevice(id);}
    @Override public void finish(){if(pendingWrites>0){closeAfterWrites=true;return;}super.finish();}
    @Override void writesFinished(boolean ok){if(!ok)closeAfterWrites=false;if(closeAfterWrites&&pendingWrites==0)super.finish();}
    @Override protected void onDestroy(){if(manager!=null)manager.unregisterInputDeviceListener(this);if(input!=null)input.releaseAll();super.onDestroy();}
}
