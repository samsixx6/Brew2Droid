package io.github.brew.handset;

import android.app.Activity;
import android.graphics.Rect;
import android.view.*;
import android.widget.FrameLayout;
import android.widget.PopupMenu;
import java.util.function.IntConsumer;

/** Popup Android na thread da UI; apenas ações discretas atravessam a ponte nativa. */
final class InGameMenu {
    final Activity activity;final IntConsumer action;View anchor;PopupMenu popup;
    InGameMenu(Activity activity,IntConsumer action){this.activity=activity;this.action=action;}
    void show(boolean counter,boolean keypad){
        if(popup!=null)return;
        View decor=activity.getWindow().getDecorView();
        if(activity.isFinishing()||activity.isDestroyed()||!(decor instanceof ViewGroup)||!decor.isAttachedToWindow()){action.accept(0);return;}
        float density=activity.getResources().getDisplayMetrics().density;Rect visible=new Rect();decor.getWindowVisibleDisplayFrame(visible);int[] location=new int[2];decor.getLocationOnScreen(location);
        FrameLayout.LayoutParams position=new FrameLayout.LayoutParams(1,1,Gravity.TOP|Gravity.RIGHT);position.topMargin=Math.max(0,visible.top-location[1])+(int)(8*density);position.rightMargin=Math.max(0,decor.getWidth()-(visible.right-location[0]))+(int)(8*density);
        if(anchor==null){anchor=new View(activity);anchor.setAlpha(0);anchor.setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_NO);((ViewGroup)decor).addView(anchor,position);}else anchor.setLayoutParams(position);
        popup=new PopupMenu(new ContextThemeWrapper(activity,R.style.BrewTheme),anchor,Gravity.END);
        popup.getMenu().add(0,1,0,"Exit");popup.getMenu().add(0,2,1,"Display counter").setCheckable(true).setChecked(counter);popup.getMenu().add(0,3,2,"Keypad").setCheckable(true).setChecked(keypad);
        popup.setOnMenuItemClickListener(item->{int id=item.getItemId();if(id==1)action.accept(1);else{item.setChecked(!item.isChecked());action.accept(id==2?(item.isChecked()?3:2):(item.isChecked()?5:4));}return true;});
        popup.setOnDismissListener(menu->{popup=null;action.accept(0);});popup.show();
    }
    void dismiss(){if(popup!=null)popup.dismiss();}
    void dispose(){dismiss();if(anchor!=null&&anchor.getParent() instanceof ViewGroup)((ViewGroup)anchor.getParent()).removeView(anchor);anchor=null;}
}
