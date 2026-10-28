package io.github.brew.handset;

import android.app.AlertDialog;
import android.content.Intent;
import android.view.View;
import android.widget.*;
import org.json.JSONObject;
import java.util.function.Consumer;

/** Ações de perfil compartilham a mesma transação e identificação estável nas duas telas. */
final class ProfileDialogs {
    static void name(NativeUi ui,String initial,Consumer<String> accepted){
        EditText field=new EditText(ui);field.setSingleLine(true);field.setHint("Profile name");field.setText(initial);field.setSelectAllOnFocus(true);
        LinearLayout box=ui.column();box.setPadding(ui.dp(20),ui.dp(8),ui.dp(20),0);box.addView(field);
        AlertDialog dialog=new AlertDialog.Builder(ui).setTitle("Enter name").setView(box).setNegativeButton(android.R.string.cancel,null).setPositiveButton(android.R.string.ok,null).create();
        dialog.setOnShowListener(v->{field.requestFocus();dialog.getWindow().setSoftInputMode(android.view.WindowManager.LayoutParams.SOFT_INPUT_STATE_ALWAYS_VISIBLE);dialog.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener(button->{String value=field.getText().toString().trim();if(value.isEmpty()||value.codePointCount(0,value.length())>80){field.setError("Use 1 to 80 characters");return;}accepted.accept(value);dialog.dismiss();});});dialog.show();
    }
    static void update(NativeUi ui,JSONObject profile,String op,String name,Consumer<Boolean> changed){
        try{JSONObject request=ProfileTarget.command(op).put("id",profile.getString("id"));if(name!=null)request.put("name",name);
            ui.persist(()->ProfileTarget.call(ui,null,request),result->changed.accept("delete".equals(op)));
        }catch(Exception error){ui.error(error);}
    }
    static void delete(NativeUi ui,JSONObject profile,Consumer<Boolean> changed){
        if(profile.optBoolean("protected"))return;
        new AlertDialog.Builder(ui).setTitle("Delete profile?").setMessage("Delete \""+profile.optString("name")+"\"? Games linked to it will use the default profile.")
            .setNegativeButton(android.R.string.cancel,null).setPositiveButton("Delete",(d,w)->update(ui,profile,"delete",null,changed)).show();
    }
    static void actions(NativeUi ui,JSONObject profile,Consumer<Boolean> changed){
        String[] labels=profile.optBoolean("protected")?new String[]{"Set as default","Edit","Rename"}:new String[]{"Set as default","Edit","Rename","Delete"};
        new AlertDialog.Builder(ui).setItems(labels,(d,which)->{
            if(which==0)update(ui,profile,"set_default",null,changed);
            else if(which==1)ui.startActivity(new Intent(ui,ProfileSettingsActivity.class).putExtra("brew.profile_id",profile.optString("id")));
            else if(which==2)name(ui,profile.optString("name"),value->update(ui,profile,"rename",value,changed));
            else delete(ui,profile,changed);
        }).show();
    }
    static void popup(NativeUi ui,View anchor,JSONObject profile,Consumer<Boolean> changed){
        PopupMenu menu=new PopupMenu(ui,anchor);menu.getMenu().add(0,1,0,"Set as default");menu.getMenu().add(0,2,1,"Rename");menu.getMenu().add(0,3,2,"Delete").setEnabled(!profile.optBoolean("protected"));
        menu.setOnMenuItemClickListener(item->{if(item.getItemId()==1)update(ui,profile,"set_default",null,changed);else if(item.getItemId()==2)name(ui,profile.optString("name"),value->update(ui,profile,"rename",value,changed));else delete(ui,profile,changed);return true;});menu.show();
    }
}
