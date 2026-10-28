package io.github.brew.handset;

import android.app.AlertDialog;
import android.graphics.Color;
import android.graphics.drawable.ColorDrawable;
import android.os.*;
import android.view.*;
import android.widget.*;
import org.json.*;

/** Alterações ficam num buffer até Save; apenas uma escrita aceita converte o jogo em Custom. */
public final class ButtonLayoutActivity extends NativeUi {
    ProfileTarget target;ButtonLayoutModel model;ButtonLayoutView canvas;TextView selected,size;SeekBar slider;LinearLayout inspector;
    boolean leaving,closeAfterWrites;Object backCallback;PopupWindow hidePopup;
    @Override public void onCreate(Bundle saved){
        super.onCreate(saved);target=new ProfileTarget(this);root.addView(toolbar("Customize buttons",true));
        if(Build.VERSION.SDK_INT>=33)backCallback=Back33.register(this);
        load(()->work(()->target.query(this),state->{try{initialize(state,saved);}catch(Exception e){error(e);}}));
    }
    void initialize(JSONObject state,Bundle saved)throws Exception{
        JSONObject values=state.getJSONObject("values"),layout=values.optJSONObject("layout");
        model=new ButtonLayoutModel();loadLayout(layout);model.markSaved();
        if(saved!=null&&saved.containsKey("brew.layout.buffer"))loadLayout(new JSONObject(saved.getString("brew.layout.buffer")));
        if(saved!=null){model.dirty=saved.getBoolean("brew.layout.dirty",false);model.selected=Math.max(-1,Math.min(20,saved.getInt("brew.layout.selected",-1)));}
        root.removeAllViews();LinearLayout bar=toolbar("Customize buttons",true);Button hide=button("Hide",()->showHide());hide.setTextColor(accent);hide.setBackgroundColor(Color.TRANSPARENT);hide.setTag("brew.layout.hide");bar.addView(hide,new LinearLayout.LayoutParams(dp(68),dp(56)));
        Button save=button("Save",this::saveLayout);save.setTag("brew.layout.save");save.setTextColor(accent);save.setBackgroundColor(Color.TRANSPARENT);bar.addView(save,new LinearLayout.LayoutParams(dp(64),dp(56)));
        ImageButton more=iconButton("more","More button options",()->{});more.setTag("brew.layout.more");more.setOnClickListener(view->{PopupMenu menu=new PopupMenu(this,more);menu.getMenu().add("Reset layout").setOnMenuItemClickListener(item->{if(model!=null&&pendingWrites==0){model.reset();updateControls();canvas.invalidate();}return true;});menu.getMenu().add("Load Preset").setOnMenuItemClickListener(item->{showPresets();return true;});menu.show();});bar.addView(more,new LinearLayout.LayoutParams(dp(48),dp(56)));root.addView(bar);
        FrameLayout editor=new FrameLayout(this);editor.setTag("brew.layout.editor");root.addView(editor,new LinearLayout.LayoutParams(-1,0,1));
        canvas=new ButtonLayoutView(this,model,values.optInt("transparency",50),state.optString("profile_name","Default"),this::updateControls);canvas.setTag("brew.layout.canvas");editor.addView(canvas,new FrameLayout.LayoutParams(-1,-1));
        inspector=column();inspector.setTag("brew.layout.inspector");inspector.setPadding(dp(16),dp(10),dp(16),dp(10));inspector.setBackgroundColor((panel&0x00ffffff)|0xd9000000);inspector.setElevation(dp(6));inspector.setClickable(true);
        FrameLayout.LayoutParams overlay=new FrameLayout.LayoutParams(-1,-2,Gravity.TOP);overlay.setMargins(dp(8),dp(8),dp(8),0);editor.addView(inspector,overlay);
        selected=label("",14,text);inspector.addView(selected);LinearLayout sizes=new LinearLayout(this);sizes.setPadding(0,dp(8),0,0);sizes.addView(label("Button size",14,text),new LinearLayout.LayoutParams(0,-2,1));size=label("",14,muted);sizes.addView(size);inspector.addView(sizes);
        slider=new SeekBar(this);slider.setContentDescription("Selected button size");slider.setMax(275);slider.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener(){public void onStartTrackingTouch(SeekBar s){}public void onProgressChanged(SeekBar s,int amount,boolean user){if(user&&model.selected>=0){model.setScale(model.selected,(amount+25)/100f);updateControls();canvas.invalidate();}}public void onStopTrackingTouch(SeekBar s){}});inspector.addView(slider,new LinearLayout.LayoutParams(-1,dp(40)));
        updateControls();
        String orientation=values.optString("orientation","auto");int requested="landscape".equals(orientation)?0:"portrait".equals(orientation)?1:-1;if(getRequestedOrientation()!=requested)setRequestedOrientation(requested);
    }
    void loadLayout(JSONObject layout){
        JSONObject geometry=layout==null?null:layout.optJSONObject("_geometry");
        model.configure(geometry!=null&&geometry.optInt("version")==2?geometry.optString("preset","default"):layout==null||layout.length()==0?"default":null);
        for(int i=0;i<ButtonLayoutModel.COUNT;i++){JSONObject p=layout==null?null:layout.optJSONObject(ButtonLayoutModel.IDS[i]);model.load(i,p!=null&&p.has("x")?(float)p.optDouble("x"):null,p!=null&&p.has("y")?(float)p.optDouble("y"):null,p==null?1:(float)p.optDouble("scale",1),p==null?model.presetVisible(i):p.optBoolean("visible",model.presetVisible(i)));}}
    // A sobreposição mantém a área de edição fixa sem reservar um rodapé para o inspetor.
    void updateControls(){if(model==null||selected==null)return;inspector.setVisibility(model.selected>=0?View.VISIBLE:View.GONE);if(model.selected<0)return;int percent=Math.round(model.values[model.selected].scale*100);selected.setText("Selected button: "+ButtonLayoutModel.LABELS[model.selected]);size.setText(percent+"%");slider.setProgress(percent-25);}
    JSONObject layoutJson()throws JSONException{
        JSONObject result=new JSONObject();if(model.preset!=null)result.put("_geometry",new JSONObject().put("version",2).put("preset",model.preset));for(int i=0;i<ButtonLayoutModel.COUNT;i++){ButtonLayoutModel.Placement p=model.values[i];JSONObject value=new JSONObject().put("scale",p.scale).put("visible",p.visible);if(p.x!=null)value.put("x",p.x);if(p.y!=null)value.put("y",p.y);result.put(ButtonLayoutModel.IDS[i],value);}return result;
    }
    void saveLayout(){
        if(model==null||pendingWrites>0)return;canvas.cancelGesture();if(!model.hasChanges()){leaving=true;super.finish();return;}
        try{JSONObject patch=new JSONObject().put("layout",layoutJson());persist(()->target.patch(this,patch),state->{model.dirty=false;leaving=true;setResult(RESULT_OK);super.finish();});}catch(Exception e){error(e);}
    }
    void showHide(){
        if(model==null)return;canvas.cancelGesture();View anchor=root.findViewWithTag("brew.layout.hide");ListView list=new ListView(this);list.setChoiceMode(ListView.CHOICE_MODE_MULTIPLE);
        java.util.ArrayList<Integer> indices=new java.util.ArrayList<>();java.util.ArrayList<String> labels=new java.util.ArrayList<>();
        for(int i=0;i<ButtonLayoutModel.COUNT;i++)if(model.preset==null||i!=9&&i!=15){indices.add(i);labels.add(ButtonLayoutModel.LABELS[i]);}
        list.setAdapter(new ArrayAdapter<>(this,android.R.layout.simple_list_item_multiple_choice,labels));for(int i=0;i<indices.size();i++)list.setItemChecked(i,model.values[indices.get(i)].visible);
        hidePopup=new PopupWindow(list,dp(240),Math.max(dp(120),Math.min(dp(48*ButtonLayoutModel.COUNT),(int)(root.getHeight()*.7f))),true);hidePopup.setBackgroundDrawable(new ColorDrawable(Color.WHITE));hidePopup.setElevation(dp(8));hidePopup.setOutsideTouchable(true);hidePopup.setInputMethodMode(PopupWindow.INPUT_METHOD_NOT_NEEDED);
        list.setOnItemClickListener((parent,view,index,id)->{model.setVisible(indices.get(index),list.isItemChecked(index));updateControls();canvas.invalidate();});hidePopup.showAsDropDown(anchor,0,0,Gravity.END);
    }
    void showPresets(){
        if(model==null||pendingWrites>0)return;canvas.cancelGesture();
        new AlertDialog.Builder(this).setTitle("Load Preset").setItems(new String[]{"Default","D-Pad","NumPad"},(dialog,index)->{model.applyPreset(new String[]{"default","dpad","numpad"}[index]);updateControls();canvas.invalidate();}).setNegativeButton("Cancel",null).show();
    }
    @Override public void finish(){
        if(pendingWrites>0){closeAfterWrites=true;return;}
        if(model!=null&&model.hasChanges()&&!leaving){new AlertDialog.Builder(this).setTitle("Discard button changes?").setNegativeButton("Keep editing",null).setPositiveButton("Discard",(d,w)->{leaving=true;super.finish();}).show();return;}super.finish();
    }
    @Override public void onBackPressed(){finish();}
    @Override void writesFinished(boolean ok){if(!ok)closeAfterWrites=false;if(closeAfterWrites&&pendingWrites==0)finish();}
    @Override protected void onPause(){if(canvas!=null)canvas.cancelGesture();super.onPause();}
    @Override protected void onSaveInstanceState(Bundle saved){if(model!=null){model.cancel();try{saved.putString("brew.layout.buffer",layoutJson().toString());saved.putBoolean("brew.layout.dirty",model.dirty);saved.putInt("brew.layout.selected",model.selected);}catch(Exception ignored){}}super.onSaveInstanceState(saved);}
    @Override protected void onDestroy(){if(hidePopup!=null)hidePopup.dismiss();if(canvas!=null)canvas.release();if(backCallback!=null)Back33.unregister(this,backCallback);super.onDestroy();}
    private static final class Back33 {
        static Object register(ButtonLayoutActivity activity){android.window.OnBackInvokedCallback callback=activity::finish;activity.getOnBackInvokedDispatcher().registerOnBackInvokedCallback(android.window.OnBackInvokedDispatcher.PRIORITY_DEFAULT,callback);return callback;}
        static void unregister(ButtonLayoutActivity activity,Object callback){activity.getOnBackInvokedDispatcher().unregisterOnBackInvokedCallback((android.window.OnBackInvokedCallback)callback);}
    }
}
