package io.github.brew.handset;

public final class GameMenuBoundaryTest {
    private static native int takeMenu();
    private static void check(boolean ok,String message){if(!ok)throw new AssertionError(message);}
    public static void main(String[] args){
        GameActivity activity=new GameActivity();activity.onCreate(new android.os.Bundle());
        check(takeMenu()==0,"Menu queue starts empty");
        activity.showGameMenu(true,false);InGameMenu menu=InGameMenu.last;
        check(menu.open&&menu.counter&&!menu.keypad,"Checked states did not reach UI");
        menu.choose(2);check(takeMenu()==(2|4),"Counter off plus dismissal did not cross JNI");
        check(!activity.isFinishing()&&takeMenu()==0,"Counter change closed Activity or replayed");
        activity.showGameMenu(false,true);check(!menu.counter&&menu.keypad,"Reopened menu has stale checks");
        menu.choose(4);check(takeMenu()==(2|8),"Keypad off did not cross JNI");
        activity.showGameMenu(false,false);menu.choose(5);check(takeMenu()==(2|8|32),"Keypad on state lost");
        activity.showGameMenu(false,true);menu.choose(3);check(takeMenu()==(2|4|16),"Counter on state lost");
        activity.showGameMenu(true,true);activity.dismissGameMenu();check(takeMenu()==2,"Back dismissal was not delivered");
        activity.showGameMenu(true,true);menu.choose(1);check(takeMenu()==3,"Exit was not delivered");
        activity.showGameMenu(true,true);activity.onDestroy();
        check(menu.disposed&&!menu.open&&takeMenu()==2,"Destroyed Activity leaked its popup");
        System.out.println("PASS: real GameActivity/native menu JNI host boundary");
    }
}
