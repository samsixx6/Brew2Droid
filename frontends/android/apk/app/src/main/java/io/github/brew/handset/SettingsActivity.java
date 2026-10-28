package io.github.brew.handset;

import android.app.*;
import android.content.*;
import android.graphics.drawable.*;
import android.os.Bundle;
import android.view.*;
import android.widget.*;
import org.json.*;
import java.io.File;

/** Somente opções que chegam ao caminho BREW móvel. Nenhuma sessão é criada aqui. */
public final class SettingsActivity extends NativeUi {
    static final int FOLDER=10,BANK=11;
    LinearLayout body;boolean closeAfterWrites,rebuildAfterWrites;
    @Override public void onCreate(Bundle saved){super.onCreate(saved);load(this::build);}
    Object value(String path){String[] parts=path.split("\\.");JSONObject at=settings;for(int i=0;i<parts.length-1;i++){at=at.optJSONObject(parts[i]);if(at==null)return null;}return at.opt(parts[parts.length-1]);}
    boolean checked(String path){return path.equals("controles_na_tela.modo")?!"nunca".equals(value(path)):Boolean.TRUE.equals(value(path));}
    void edit(String path,Object value,boolean rebuild){try{rebuildAfterWrites|=rebuild;JSONObject patch=new JSONObject(),at=patch;String[] parts=path.split("\\.");for(int i=0;i<parts.length-1;i++){JSONObject child=new JSONObject();at.put(parts[i],child);at=child;}at.put(parts[parts.length-1],path.equals("controles_na_tela.modo")?(Boolean.TRUE.equals(value)?"automatico":"nunca"):value==null?JSONObject.NULL:value);
        persist(()->unwrap(NativeBridge.update(config(),defaultRoot(),patch.toString())),v->{try{accept(v);if(pendingWrites==0&&rebuildAfterWrites){rebuildAfterWrites=false;build();}}catch(Exception e){error(e);}});
    }catch(Exception e){error(e);}}
    @Override void error(Throwable e){super.error(e);if(settings.length()>0)build();}
    @Override public void finish(){if(pendingWrites>0){closeAfterWrites=true;return;}super.finish();}
    @Override void writesFinished(boolean ok){if(!ok)closeAfterWrites=false;if(closeAfterWrites&&pendingWrites==0)super.finish();}
    void toggle(String path,String title,String help,boolean enabled,boolean rebuild){
        String symbol=(path.startsWith("controles")||path.startsWith("handset_keypad"))?"controls":path.equals("graphics.smooth")?"image":path.startsWith("graphics")?"screen":path.equals("audio.midi_effects")?"music":path.startsWith("audio")?"audio":"performance";
        LinearLayout row=new LinearLayout(this);row.setGravity(Gravity.CENTER_VERTICAL);row.setPadding(dp(16),dp(12),dp(16),dp(12));row.setMinimumHeight(dp(64));row.setBackgroundColor(panel);
        row.addView(icon(symbol,text),new LinearLayout.LayoutParams(dp(36),dp(32)));
        LinearLayout words=column();words.setPadding(dp(12),0,dp(8),0);words.addView(label(title,16,text));if(help!=null){TextView detail=label(help,13,muted);detail.setPadding(0,dp(3),0,0);words.addView(detail);}row.addView(words,new LinearLayout.LayoutParams(0,-2,1));
        Switch control=new Switch(this);control.setTag("brew.setting."+path);control.setContentDescription(title);control.setChecked(checked(path));control.setEnabled(enabled);words.setAlpha(enabled?1:.45f);control.setPadding(dp(12),0,0,0);
        StateListDrawable track=new StateListDrawable();track.addState(new int[]{-android.R.attr.state_enabled},switchTrack(0xffdddddf));track.addState(new int[]{android.R.attr.state_checked},switchTrack(accent));track.addState(new int[]{},switchTrack(0xffc5c5c9));
        control.setTrackTintList(null);control.setTrackDrawable(track);GradientDrawable thumb=new GradientDrawable();thumb.setShape(GradientDrawable.OVAL);thumb.setColor(android.graphics.Color.WHITE);thumb.setStroke(dp(1),0xffdddddf);thumb.setSize(dp(24),dp(24));control.setThumbTintList(null);control.setThumbDrawable(thumb);control.setSplitTrack(false);control.setSwitchMinWidth(dp(44));
        control.setOnCheckedChangeListener((b,on)->edit(path,on,rebuild));row.addView(control,new LinearLayout.LayoutParams(-2,-2));row.setOnClickListener(v->{if(control.isEnabled())control.performClick();});body.addView(row,new LinearLayout.LayoutParams(-1,-2));divider(body);
    }
    GradientDrawable switchTrack(int color){GradientDrawable track=new GradientDrawable();track.setColor(color);track.setCornerRadius(dp(14));track.setSize(dp(44),dp(24));return track;}
    void choose(String title,String[] labels,String[] values,String selected,String field){int index=0;for(int i=0;i<values.length;i++)if(values[i].equals(selected))index=i;
        new AlertDialog.Builder(this).setTitle(title).setSingleChoiceItems(labels,index,(d,which)->{d.dismiss();edit(field,values[which],true);}).setNegativeButton(android.R.string.cancel,null).show();}
    void picker(int request,String path,boolean file){startActivityForResult(new Intent(this,FolderPickerActivity.class).putExtra("brew.path",path).putExtra("brew.file_mode",file),request);}
    void build(){int scrollY=0;View old=root.findViewWithTag("brew.settings.scroll");if(old instanceof ScrollView)scrollY=old.getScrollY();root.removeAllViews();LinearLayout bar=toolbar(tr("native.settings"),true);ImageButton more=iconButton("more",tr("native.more"),()->{});more.setOnClickListener(v->{PopupMenu menu=new PopupMenu(this,more);menu.getMenu().add(0,1,0,"Profiles");menu.getMenu().add(0,2,1,tr("library.rescan"));menu.setOnMenuItemClickListener(item->{if(item.getItemId()==1)openProfiles();else{setResult(RESULT_OK);finish();}return true;});menu.show();});bar.addView(more,new LinearLayout.LayoutParams(dp(48),dp(56)));root.addView(bar);
        ScrollView scroll=new ScrollView(this);scroll.setTag("brew.settings.scroll");body=column();body.setPadding(0,0,0,dp(24));scroll.addView(body);root.addView(scroll,new LinearLayout.LayoutParams(-1,0,1));
        section(body,tr("native.library"));String folder=settings.optString("roms_dir",defaultRoot());
        row(body,"folder",tr("native.games_folder"),folder.startsWith(getFilesDir().toString())?tr("native.imported"):folder.replaceFirst("^/storage/emulated/0/?","Internal storage / "),()->picker(FOLDER,folder,false));
        String language=data.optString("language","en"),languageName=language;JSONArray available=data.optJSONArray("languages");for(int i=0;available!=null&&i<available.length();i++){JSONObject entry=available.optJSONObject(i);if(language.equals(entry.optString("code")))languageName=entry.optString("name");}
        row(body,"language",tr("native.app_language"),languageName,()->{
            JSONArray languages=data.optJSONArray("languages");String[] names=new String[languages.length()],codes=new String[languages.length()];for(int i=0;i<names.length;i++){names[i]=languages.optJSONObject(i).optString("name");codes[i]=languages.optJSONObject(i).optString("code");}
            choose(tr("settings.language"),names,codes,data.optString("language"),"language");
        });
        section(body,tr("native.display"));
        row(body,"screen","Display Renderer",checked("graphics.gpu_rasterizer")?"GPU":"CPU",()->new AlertDialog.Builder(this).setTitle("Display Renderer").setSingleChoiceItems(new String[]{"CPU","GPU"},checked("graphics.gpu_rasterizer")?1:0,(d,i)->{d.dismiss();edit("graphics.gpu_rasterizer",i==1,true);}).setNegativeButton(android.R.string.cancel,null).show());
        section(body,tr("settings.tab.controls"));
        row(body,"controls","Profiles","Game display, buttons and layouts",this::openProfiles);
        toggle("handset_keypad.vibration","Enable vibration",null,true,false);
        section(body,tr("settings.tab.audio"));toggle("audio.enabled",tr("audio.enabled"),null,true,false);
        int volume=settings.optJSONObject("audio").optInt("volume",100);LinearLayout volumeRow=new LinearLayout(this);volumeRow.setPadding(dp(16),dp(12),dp(16),0);volumeRow.setGravity(Gravity.CENTER_VERTICAL);volumeRow.setBackgroundColor(panel);volumeRow.addView(icon("volume",text),new LinearLayout.LayoutParams(dp(36),dp(32)));TextView volumeTitle=label(tr("audio.volume"),16,text);volumeTitle.setPadding(dp(12),0,0,0);volumeRow.addView(volumeTitle,new LinearLayout.LayoutParams(0,-2,1));TextView volumeLabel=label(volume+"%",14,muted);volumeRow.addView(volumeLabel);body.addView(volumeRow);
        SeekBar seek=new SeekBar(this);seek.setTag("brew.setting.audio.volume");seek.setContentDescription(tr("audio.volume"));seek.setMax(100);seek.setProgress(volume);seek.setPadding(dp(16),dp(8),dp(16),dp(8));
        seek.setPadding(dp(64),dp(4),dp(16),dp(8));seek.setBackgroundColor(panel);seek.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener(){public void onStartTrackingTouch(SeekBar b){}public void onProgressChanged(SeekBar b,int n,boolean user){volumeLabel.setText(n+"%");}public void onStopTrackingTouch(SeekBar b){edit("audio.volume",b.getProgress(),false);}});
        seek.setOnKeyListener((v,key,e)->{if(e.getAction()==android.view.KeyEvent.ACTION_UP)edit("audio.volume",seek.getProgress(),false);return false;});body.addView(seek,new LinearLayout.LayoutParams(-1,dp(44)));divider(body);
        String bank=settings.optJSONObject("audio").isNull("soundfont")?"":settings.optJSONObject("audio").optString("soundfont","");row(body,"music",tr("audio.soundfont"),bank.isEmpty()?tr("handset.audio.soundfont.auto"):bank,()->picker(BANK,bank.isEmpty()?"/sdcard":new File(bank).getParent(),true));hint(body,tr("handset.audio.soundfont.hint"));
        if(!bank.isEmpty())row(body,tr("audio.soundfont.clear"),null,()->edit("audio.soundfont",null,true));
        toggle("audio.midi_effects",tr("audio.midi_effects"),tr("handset.audio.midi_effects.hint"),true,false);
        section(body,tr("native.performance"));toggle("debug.fps_counter",tr("debug.fps_counter"),null,true,false);
        section(body,tr("settings.tab.about"));hint(body,"Brew2Droid\n"+tr("about.version").replace("{version}",data.optString("version")));
        row(body,tr("about.discord"),null,()->startActivity(new Intent(Intent.ACTION_VIEW,android.net.Uri.parse("https://discord.gg/Bv8znZ626"))));
        final int y=scrollY;scroll.post(()->scroll.scrollTo(0,y));
    }
    @Override protected void onActivityResult(int req,int result,Intent data){super.onActivityResult(req,result,data);if(result==RESULT_OK&&data!=null){String path=data.getStringExtra("brew.path");if(path!=null)edit(req==FOLDER?"roms_dir":"audio.soundfont",path,true);}}
}
