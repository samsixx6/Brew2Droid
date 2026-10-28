package io.github.brew.handset;

import android.app.AlertDialog;
import android.content.Intent;
import android.os.Bundle;
import android.text.InputType;
import android.view.*;
import android.widget.*;
import org.json.*;

/** Ajustes efetivos iguais nas telas de perfil e jogo; somente o alvo da escrita muda. */
abstract class OptionsActivity extends NativeUi {
    ProfileTarget target;JSONObject state=new JSONObject(),values=new JSONObject();LinearLayout body;
    boolean loaded,closeAfterWrites;int scrollY;
    final OptionEdits edits=new OptionEdits();boolean writeFailed;
    void prepare()throws Exception{}
    void identity(LinearLayout body){}
    String heading(){return target.game()?"Game settings":state.optString("name","Profile");}
    JSONObject readTarget()throws Exception{return target.query(this);}
    @Override public void onCreate(Bundle saved){super.onCreate(saved);target=new ProfileTarget(this);if(saved!=null)scrollY=saved.getInt("brew.options.scroll",0);root.addView(toolbar(target.game()?"Game settings":"Profile",true));try{prepare();load(this::refresh);}catch(Exception e){error(e);}}
    @Override protected void onResume(){super.onResume();if(loaded&&pendingWrites==0)refresh();}
    void refresh(){work(this::readTarget,this::acceptTarget);}
    void acceptTarget(JSONObject result){if(pendingWrites>0)return;state=result;values=result.optJSONObject("values");if(values==null){error(new IllegalStateException("Profile has no settings"));return;}loaded=true;edits.clear();build();}
    void edit(String field,Object value){try{if(!edits.record(field,String.valueOf(values.opt(field)),String.valueOf(value)))return;JSONObject patch=new JSONObject().put(field,value==null?JSONObject.NULL:value);persist(()->target.patch(this,patch),result->{setResult(RESULT_OK);acceptTarget(result);});}catch(Exception e){error(e);}}
    void choose(String title,String field,String[] names,String[] codes){int selected=0;for(int i=0;i<codes.length;i++)if(codes[i].equals(values.optString(field)))selected=i;
        new AlertDialog.Builder(this).setTitle(title).setSingleChoiceItems(names,selected,(d,index)->{d.dismiss();edit(field,codes[index]);}).setNegativeButton(android.R.string.cancel,null).show();}
    void toggle(String field,String title,String symbol,boolean fallback){preferenceSwitch(body,symbol,title,values.optBoolean(field,fallback),value->edit(field,value),"brew.profile.setting."+field);}
    void build(){
        View old=root.findViewWithTag("brew.options.scroll");if(old instanceof ScrollView)scrollY=old.getScrollY();root.removeAllViews();LinearLayout bar=toolbar(heading(),true);
        ImageButton more=iconButton("more","More options",()->{});more.setOnClickListener(v->{if(pendingWrites>0)return;if(target.game())gameMenu(more);else ProfileDialogs.popup(this,more,state,deleted->{if(deleted)finish();else refresh();});});bar.addView(more,new LinearLayout.LayoutParams(dp(48),dp(56)));root.addView(bar);
        ScrollView scroll=new ScrollView(this);scroll.setTag("brew.options.scroll");body=column();body.setPadding(0,0,0,dp(24));scroll.addView(body);root.addView(scroll,new LinearLayout.LayoutParams(-1,0,1));identity(body);
        section(body,"Display");
        row(body,"screen","Resolution",resolutionLabel(),this::resolution);
        row(body,"screen","Screen orientation",labelOf(values.optString("orientation")),()->choose("Screen orientation","orientation",new String[]{"Auto","Portrait","Landscape"},new String[]{"auto","portrait","landscape"}));
        row(body,"screen","Screen gravity",labelOf(values.optString("gravity")),()->choose("Screen gravity","gravity",new String[]{"Top","Center","Bottom"},new String[]{"top","center","bottom"}));
        row(body,"screen","Screen scaling",scalingLabel(),()->choose("Screen scaling","scaling",new String[]{"Fit the window","Integer scaling","Stretch"},new String[]{"fit","integer","stretch"}));
        if("stretch".equals(values.optString("scaling")))toggle("keep_aspect","Keep aspect ratio","screen",true);
        toggle("smooth","Smooth image","image",false);
        section(body,"Controls");toggle("controls_enabled","Enable on-screen buttons","controls",true);
        row(body,"controls","Button Type",zeebo()?"Zeebo":"Cellphone",this::buttonType);transparency();
        if(!zeebo())row(body,"controls","Customize on-screen buttons",null,()->startActivity(target.into(new Intent(this,ButtonLayoutActivity.class))));
        row(body,"controls","Button Mapping","Keyboard and controller inputs",()->startActivity(target.into(new Intent(this,ButtonMappingActivity.class)).putExtra("brew.strings",strings.toString())));
        volumeKeysPreference();
        final int y=scrollY;scroll.post(()->scroll.scrollTo(0,y));
    }
    void volumeKeysPreference(){
        LinearLayout row=new LinearLayout(this);row.setGravity(Gravity.CENTER_VERTICAL);row.setPadding(dp(16),dp(12),dp(16),dp(12));row.setMinimumHeight(dp(64));row.setBackgroundColor(panel);row.addView(icon("controls",text),new LinearLayout.LayoutParams(dp(36),dp(32)));
        TextView title=label("Make volume keys mappable",16,text);title.setPadding(dp(12),0,dp(8),0);row.addView(title,new LinearLayout.LayoutParams(0,-2,1));
        CheckBox check=new CheckBox(this);check.setTag("brew.profile.setting.volume_keys_mappable");check.setContentDescription("Make volume keys mappable");check.setChecked(values.optBoolean("volume_keys_mappable",false));check.setOnCheckedChangeListener((button,on)->edit("volume_keys_mappable",on));row.addView(check);row.setOnClickListener(v->check.performClick());body.addView(row);divider(body);
    }
    String labelOf(String value){return value.isEmpty()?"Auto":Character.toUpperCase(value.charAt(0))+value.substring(1);}
    String scalingLabel(){switch(values.optString("scaling")){case "integer":return "Integer scaling";case "stretch":return "Stretch";default:return "Fit the window";}}
    boolean zeebo(){return "zeebo".equals(state.optString("button_type",values.optString("button_type","cellphone")));}
    void buttonType(){new AlertDialog.Builder(this).setTitle("Button Type").setSingleChoiceItems(new String[]{"Cellphone","Zeebo"},zeebo()?1:0,(d,index)->{d.dismiss();edit("button_type",index==1?"zeebo":"cellphone");}).setNegativeButton(android.R.string.cancel,null).show();}
    String autoResolution(){return zeebo()?"Auto otherwise 640x480":"Auto otherwise 240x320";}
    String resolutionLabel(){JSONObject size=values.optJSONObject("resolution");return size==null?autoResolution():size.optInt("width",240)+"×"+size.optInt("height",320);}
    void transparency(){
        int amount=values.optInt("transparency",50);LinearLayout title=new LinearLayout(this);title.setPadding(dp(16),dp(12),dp(16),0);title.setBackgroundColor(panel);title.addView(label("Button transparency",16,text),new LinearLayout.LayoutParams(0,-2,1));TextView percent=label(amount+"%",14,muted);title.addView(percent);body.addView(title);
        SeekBar seek=new SeekBar(this);seek.setContentDescription("Button transparency");seek.setTag("brew.profile.setting.transparency");seek.setMax(100);seek.setProgress(amount);seek.setPadding(dp(16),dp(4),dp(16),dp(8));seek.setBackgroundColor(panel);
        seek.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener(){public void onStartTrackingTouch(SeekBar s){}public void onProgressChanged(SeekBar s,int value,boolean user){percent.setText(value+"%");}public void onStopTrackingTouch(SeekBar s){edit("transparency",s.getProgress());}});seek.setOnKeyListener((v,key,event)->{if(event.getAction()==KeyEvent.ACTION_UP)edit("transparency",seek.getProgress());return false;});body.addView(seek,new LinearLayout.LayoutParams(-1,dp(44)));divider(body);
    }
    void resolution(){
        String[] labels={autoResolution(),"128×160","176×208","176×220","240×320","320×240","320×480","480×800","800×480","Custom"};
        JSONObject current=values.optJSONObject("resolution");int selected=0;if(current!=null){selected=labels.length-1;for(int i=1;i<labels.length-1;i++)if(labels[i].equals(resolutionLabel()))selected=i;}
        new AlertDialog.Builder(this).setTitle("Resolution").setSingleChoiceItems(labels,selected,(d,index)->{d.dismiss();if(index==0)edit("resolution",JSONObject.NULL);else if(index==labels.length-1)customResolution();else{String[] sides=labels[index].split("×");try{edit("resolution",new JSONObject().put("width",Integer.parseInt(sides[0])).put("height",Integer.parseInt(sides[1])).put("depth",16));}catch(Exception e){error(e);}}}).setNegativeButton(android.R.string.cancel,null).show();
    }
    void customResolution(){
        LinearLayout fields=column();fields.setPadding(dp(20),0,dp(20),0);JSONObject chosen=values.optJSONObject("resolution");JSONObject current=chosen==null?state.optJSONObject("automatic_resolution"):chosen;EditText width=new EditText(this),height=new EditText(this);EditText[] inputs={width,height};String[] keys={"width","height"},hints={"Width","Height"};
        for(int i=0;i<2;i++){inputs[i].setSingleLine(true);inputs[i].setInputType(InputType.TYPE_CLASS_NUMBER);inputs[i].setHint(hints[i]);inputs[i].setContentDescription(hints[i]);inputs[i].setText(Integer.toString(current==null?(i==0?240:320):current.optInt(keys[i],i==0?240:320)));fields.addView(inputs[i]);}
        AlertDialog dialog=new AlertDialog.Builder(this).setTitle("Custom resolution").setView(fields).setNegativeButton(android.R.string.cancel,null).setPositiveButton(android.R.string.ok,null).create();dialog.setOnShowListener(v->dialog.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener(button->{try{int w=Integer.parseInt(width.getText().toString()),h=Integer.parseInt(height.getText().toString());if(w<1||w>1024){width.setError("Use 1 to 1024");return;}if(h<1||h>1024){height.setError("Use 1 to 1024");return;}edit("resolution",new JSONObject().put("width",w).put("height",h).put("depth",16));dialog.dismiss();}catch(Exception e){width.setError("Use 1 to 1024");}}));dialog.show();
    }
    void gameMenu(View anchor){PopupMenu menu=new PopupMenu(this,anchor);menu.getMenu().add(0,1,0,"Reset settings");menu.getMenu().add(0,2,1,"Load profile");menu.setOnMenuItemClickListener(item->{if(item.getItemId()==1)persist(()->ProfileTarget.call(this,target.module,ProfileTarget.command("game_reset")),value->{setResult(RESULT_OK);acceptTarget(value);});else loadProfile();return true;});menu.show();}
    void loadProfile(){work(()->ProfileTarget.call(this,null,ProfileTarget.command("list")),catalog->{JSONArray profiles=catalog.optJSONArray("profiles");if(profiles==null)return;String[] names=new String[profiles.length()];int selected=-1;for(int i=0;i<names.length;i++){JSONObject p=profiles.optJSONObject(i);names[i]=p.optString("name");if(p.optString("id").equals(state.optString("profile_id")))selected=i;}new AlertDialog.Builder(this).setTitle("Load profile").setSingleChoiceItems(names,selected,(d,index)->{try{JSONObject request=ProfileTarget.command("game_profile").put("id",profiles.getJSONObject(index).getString("id"));d.dismiss();persist(()->ProfileTarget.call(this,target.module,request),value->{setResult(RESULT_OK);acceptTarget(value);});}catch(Exception e){error(e);}}).setNegativeButton(android.R.string.cancel,null).show();});}
    @Override public void finish(){if(pendingWrites>0){closeAfterWrites=true;return;}super.finish();}
    @Override void writesFinished(boolean ok){writeFailed|=!ok;if(!ok)closeAfterWrites=false;if(pendingWrites==0){edits.clear();if(writeFailed){writeFailed=false;refresh();}else if(closeAfterWrites)super.finish();}}
    @Override protected void onSaveInstanceState(Bundle saved){View scroll=root.findViewWithTag("brew.options.scroll");saved.putInt("brew.options.scroll",scroll==null?scrollY:scroll.getScrollY());super.onSaveInstanceState(saved);}
}
