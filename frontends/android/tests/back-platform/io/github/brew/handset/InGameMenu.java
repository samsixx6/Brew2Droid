package io.github.brew.handset;
import java.util.function.IntConsumer;

/** Fronteira de UI modelada; a Activity e os exports JNI são os do aplicativo. */
final class InGameMenu {
    static InGameMenu last; final IntConsumer action; boolean open,counter,keypad,disposed;
    InGameMenu(GameActivity activity,IntConsumer action){this.action=action;last=this;}
    void show(boolean counter,boolean keypad){if(open)return;this.counter=counter;this.keypad=keypad;open=true;}
    void choose(int action){if(!open)throw new AssertionError("Menu is closed");this.action.accept(action);dismiss();}
    void dismiss(){if(open){open=false;action.accept(0);}}
    void dispose(){dismiss();disposed=true;}
}
