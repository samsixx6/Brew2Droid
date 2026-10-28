package io.github.brew.handset;

import android.app.Activity;
import android.content.Context;
import android.content.Intent;
import android.os.Environment;
import java.io.File;
import org.json.JSONObject;

/** O mesmo alvo serve aos ajustes, mapeamento e editor, sem copiar vínculos para o Java. */
final class ProfileTarget {
    final String module,id;
    ProfileTarget(Activity activity){
        String path=activity.getIntent().getStringExtra("brew.game_path");
        module=path==null||path.isEmpty()?null:path;
        String profile=activity.getIntent().getStringExtra("brew.profile_id");
        id=profile==null||profile.isEmpty()?"default":profile;
    }
    boolean game(){return module!=null;}
    static String publicRoot(){return new File(Environment.getExternalStorageDirectory(),"brew2droid/profiles").toString();}
    static JSONObject command(String op){try{return new JSONObject().put("op",op);}catch(Exception e){throw new IllegalArgumentException(e);}}
    static JSONObject call(Context context,String module,JSONObject request)throws Exception{
        File files=context.getFilesDir();
        return NativeUi.unwrap(NativeBridge.profiles(publicRoot(),files.toString(),new File(files,"zeebx.json").toString(),module==null?"":module,request.toString()));
    }
    JSONObject query(Context context)throws Exception{return call(context,module,game()?command("game"):command("view").put("id",id));}
    JSONObject patch(Context context,JSONObject patch)throws Exception{return call(context,module,command(game()?"game_update":"update").put("id",id).put("patch",patch));}
    Intent into(Intent intent){if(game())intent.putExtra("brew.game_path",module);else intent.putExtra("brew.profile_id",id);return intent;}
}
