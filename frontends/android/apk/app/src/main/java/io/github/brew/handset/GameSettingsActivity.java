package io.github.brew.handset;

import android.graphics.Bitmap;
import android.graphics.Typeface;
import android.text.TextUtils;
import android.text.SpannableString;
import android.text.Spanned;
import android.text.style.ForegroundColorSpan;
import android.view.Gravity;
import android.widget.*;
import org.json.JSONObject;
import java.io.File;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.file.Files;

/** O cabeçalho mostra o vínculo vivo ou Custom; os campos são os mesmos dos perfis. */
public final class GameSettingsActivity extends OptionsActivity {
    String title,art;Bitmap image;boolean artRead;
    @Override void prepare(){
        if(!target.game())throw new IllegalArgumentException("Game module is missing");
        title=getIntent().getStringExtra("brew.game_title");art=getIntent().getStringExtra("brew.game_art");
        if(title==null||title.isEmpty())title=new File(target.module).getName();
    }
    @Override JSONObject readTarget()throws Exception{JSONObject result=super.readTarget();if(!artRead){image=lerArte(art);artRead=true;}return result;}
    @Override void identity(LinearLayout body){
        LinearLayout card=new LinearLayout(this);card.setGravity(Gravity.CENTER_VERTICAL);card.setPadding(dp(20),dp(20),dp(20),dp(20));card.setBackgroundColor(panel);
        ImageView cover=new ImageView(this);cover.setScaleType(ImageView.ScaleType.FIT_CENTER);cover.setImportantForAccessibility(android.view.View.IMPORTANT_FOR_ACCESSIBILITY_NO);
        if(image!=null)cover.setImageBitmap(image);else cover.setImageDrawable(new UiIcons("controls",muted,dp(36)));card.addView(cover,new LinearLayout.LayoutParams(dp(64),dp(64)));
        LinearLayout words=column();words.setPadding(dp(18),0,0,0);TextView name=label(title,20,text);name.setTypeface(Typeface.create("sans-serif-medium",Typeface.NORMAL));name.setMaxLines(3);name.setEllipsize(TextUtils.TruncateAt.END);words.addView(name);
        String value="Profile: "+state.optString("profile_name","Default");SpannableString colored=new SpannableString(value);colored.setSpan(new ForegroundColorSpan(accent),9,value.length(),Spanned.SPAN_EXCLUSIVE_EXCLUSIVE);
        TextView profile=label(value,14,muted);profile.setText(colored);profile.setTag("brew.game.profile");profile.setPadding(0,dp(5),0,0);words.addView(profile);card.addView(words,new LinearLayout.LayoutParams(0,-2,1));body.addView(card);divider(body);
    }

    static Bitmap lerArte(String caminhoArte) {
        if (caminhoArte == null || caminhoArte.isEmpty()) return null;
        try {
            File arquivo = new File(caminhoArte);
            if (arquivo.length() < 12 || arquivo.length() > 8 + 192 * 192 * 4) return null;
            byte[] bytes = Files.readAllBytes(arquivo.toPath());
            ByteBuffer dados = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN);
            int largura = dados.getInt(), altura = dados.getInt();
            if (largura < 1 || altura < 1 || largura > 192 || altura > 192 || bytes.length != 8 + largura * altura * 4) return null;
            int[] pixels = new int[largura * altura];
            for (int i = 0; i < pixels.length; i++) {
                int r = dados.get() & 255, g = dados.get() & 255, b = dados.get() & 255, a = dados.get() & 255;
                pixels[i] = (a << 24) | (r << 16) | (g << 8) | b;
            }
            return Bitmap.createBitmap(pixels, largura, altura, Bitmap.Config.ARGB_8888);
        } catch (Exception erro) { return null; }
    }
}
