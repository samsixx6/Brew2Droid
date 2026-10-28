package io.github.brew.handset;

import android.content.Context;
import android.app.Activity;
import java.util.*;
import org.json.*;

/** O backend salva no perfil ou snapshot Custom; nenhuma atribuição global é regravada. */
final class ButtonMappingStore {
    private final Context context;
    private final ProfileTarget target;
    private ButtonBindings accepted;
    private String profileName;
    private boolean volumeKeysMappable,zeebo;
    boolean isZeebo(){return zeebo;}
    boolean volumeKeysMappable(){return volumeKeysMappable;}
    ButtonMappingStore(Context context){this.context=context;if(!(context instanceof Activity))throw new IllegalArgumentException("Mapping needs a settings target");target=new ProfileTarget((Activity)context);accepted=read();}
    ButtonBindings current(){return accepted;}
    String profileName(){return profileName;}
    private ButtonBindings read(){
        try{
            JSONObject state=target.query(context);profileName=state.optString("profile_name","Default");
            JSONObject values=state.getJSONObject("values");volumeKeysMappable=values.optBoolean("volume_keys_mappable",false);
            zeebo="zeebo".equals(state.optString("button_type",values.optString("button_type","cellphone")));
            JSONObject entries=values.optJSONObject(zeebo?"zeebo_bindings":"bindings");if(entries==null)entries=new JSONObject();ButtonBindings result=ButtonBindings.defaults(zeebo);Set<String> used=new HashSet<>();
            for(ButtonBindings.Target target:ButtonBindings.targets(zeebo)){
                Object token=entries.opt(target.id);
                if(token instanceof String&&ButtonBindings.validToken((String)token)&&used.add((String)token))result=result.assign(target,(String)token);
            }
            return result;
        }catch(Exception e){throw new IllegalStateException("Could not read button mappings: "+e.getMessage(),e);}
    }
    boolean save(ButtonBindings bindings){
        try{
            if(bindings.isZeebo()!=zeebo)return false;
            JSONObject entries=new JSONObject();
            for(ButtonBindings.Target target:ButtonBindings.targets(zeebo))if(bindings.custom(target)!=null)entries.put(target.id,bindings.custom(target));
            JSONObject state=target.patch(context,new JSONObject().put(zeebo?"zeebo_bindings":"bindings",entries));profileName=state.optString("profile_name","Custom");accepted=bindings;return true;
        }catch(Exception e){return false;}
    }
}
