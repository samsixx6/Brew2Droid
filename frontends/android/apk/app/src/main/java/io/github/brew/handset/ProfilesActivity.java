package io.github.brew.handset;

import android.content.Intent;
import android.os.Bundle;
import android.view.Gravity;
import android.widget.*;
import org.json.*;

/** A lista lê somente o catálogo de perfis, independentemente do tamanho da biblioteca. */
public final class ProfilesActivity extends NativeUi {
    JSONObject catalog=new JSONObject();boolean loaded,closeAfterWrites;
    @Override public void onCreate(Bundle state){super.onCreate(state);root.addView(toolbar("Profiles",true));load(this::refresh);}
    @Override protected void onResume(){super.onResume();if(loaded&&pendingWrites==0)refresh();}
    void refresh(){work(()->ProfileTarget.call(this,null,ProfileTarget.command("list")),value->{catalog=value;loaded=true;build();});}
    void build(){
        root.removeAllViews();LinearLayout bar=toolbar("Profiles",true);
        ImageButton add=iconButton("add","Add profile",()->ProfileDialogs.name(this,"",name->{try{JSONObject request=ProfileTarget.command("create").put("name",name);persist(()->ProfileTarget.call(this,null,request),value->startActivity(new Intent(this,ProfileSettingsActivity.class).putExtra("brew.profile_id",value.optString("id"))));}catch(Exception e){error(e);}}));
        // UiIcons conserva os demais ícones; o + é um símbolo nativo simples.
        add.setImageDrawable(new android.graphics.drawable.Drawable(){final android.graphics.Paint paint=new android.graphics.Paint(3);public void draw(android.graphics.Canvas canvas){paint.setColor(android.graphics.Color.WHITE);paint.setStrokeWidth(dp(2));android.graphics.Rect b=getBounds();float x=b.exactCenterX(),y=b.exactCenterY();canvas.drawLine(x-dp(9),y,x+dp(9),y,paint);canvas.drawLine(x,y-dp(9),x,y+dp(9),paint);}public void setAlpha(int a){}public void setColorFilter(android.graphics.ColorFilter f){}public int getOpacity(){return android.graphics.PixelFormat.TRANSLUCENT;}});
        bar.addView(add,new LinearLayout.LayoutParams(dp(48),dp(56)));root.addView(bar);
        ScrollView scroll=new ScrollView(this);LinearLayout body=column();scroll.addView(body);root.addView(scroll,new LinearLayout.LayoutParams(-1,0,1));
        JSONArray profiles=catalog.optJSONArray("profiles");for(int i=0;profiles!=null&&i<profiles.length();i++){
            JSONObject profile=profiles.optJSONObject(i);if(profile==null)continue;
            LinearLayout row=new LinearLayout(this);row.setGravity(Gravity.CENTER_VERTICAL);row.setPadding(dp(16),0,dp(8),0);row.setMinimumHeight(dp(64));row.setBackgroundColor(panel);row.setTag("brew.profile."+profile.optString("id"));
            TextView label=label(profile.optString("name"),17,text);row.addView(label,new LinearLayout.LayoutParams(0,-2,1));
            if(profile.optBoolean("selected")){TextView check=label("✓",24,accent);check.setContentDescription("Default profile");check.setGravity(Gravity.CENTER);row.addView(check,new LinearLayout.LayoutParams(dp(40),dp(48)));}
            ImageButton more=iconButton("more","Profile actions",()->ProfileDialogs.actions(this,profile,deleted->refresh()));more.setImageDrawable(new UiIcons("more",muted,dp(24)));row.addView(more,new LinearLayout.LayoutParams(dp(48),dp(56)));
            row.setOnClickListener(v->startActivity(new Intent(this,ProfileSettingsActivity.class).putExtra("brew.profile_id",profile.optString("id"))));row.setOnLongClickListener(v->{ProfileDialogs.actions(this,profile,deleted->refresh());return true;});body.addView(row);divider(body);
        }
    }
    @Override public void finish(){if(pendingWrites>0){closeAfterWrites=true;return;}super.finish();}
    @Override void writesFinished(boolean ok){if(!ok)closeAfterWrites=false;if(closeAfterWrites&&pendingWrites==0)super.finish();}
}
