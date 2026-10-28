package io.github.brew.handset;

/** Executa a Activity real e o export JNI real, sem biblioteca Android ou sessão falsa no APK. */
public final class BackNavigationTest {
    private static native boolean takeBack();
    private static native long takePhysical();
    private static void check(boolean ok,String message){if(!ok)throw new AssertionError(message);}
    public static void main(String[] args){
        GameActivity activity=new GameActivity();activity.onCreate(new android.os.Bundle());
        check(!takeBack(),"Back queue must start empty");check(takePhysical()==-1,"Activity start must clear stale physical input");
        for(int scan:new int[]{0,1,158,217,999}){
            activity.dispatchKeyEvent(new android.view.KeyEvent(0,4,scan,1,0));
            activity.dispatchKeyEvent(new android.view.KeyEvent(0,4,scan,1,0));
            check(takePhysical()==((1L<<48)|(1L<<32)|((long)scan<<1)|1),"Physical press/repeat must cross JNI once");
            activity.dispatchKeyEvent(new android.view.KeyEvent(1,4,scan,1,0));
            check(takePhysical()==((1L<<48)|(1L<<32)|((long)scan<<1)),"Physical release lost or duplicated at JNI");
            check(!takeBack(),"Physical Back scan "+scan+" opened the Android menu");
        }
        activity.dispatchKeyEvent(new android.view.KeyEvent(0,4,217,1,0));
        activity.commitSystemBack();check(takeBack(),"Virtual/system Back must still work while a physical Back is held");
        activity.dispatchKeyEvent(new android.view.KeyEvent(1,4,217,1,0));
        activity.commitSystemBack();
        check(!activity.isFinishing(),"System Back closed the game Activity");
        check(takeBack(),"System Back did not reach the native menu");
        check(!takeBack(),"One Back opened and closed the menu");
        activity.commitSystemBack();
        check(takeBack(),"Second Back must also reach the native menu");
        takePhysical();
        for(int[] virtual:new int[][]{{0,-1,0},{158,1,64},{158,1,2},{158,99,0}}){
            activity.dispatchKeyEvent(new android.view.KeyEvent(0,4,virtual[0],virtual[1],virtual[2]));
            activity.dispatchKeyEvent(new android.view.KeyEvent(1,4,virtual[0],virtual[1],virtual[2]));
            check(takeBack(),"Virtual Back was consumed as a BREW key");
            check(takePhysical()==0,"Virtual Back entered the physical JNI queue");
        }
        activity.dispatchKeyEvent(new android.view.KeyEvent(0,4,158,1,0));activity.onPause();
        check(takePhysical()==-1,"Pause must replace pending presses with release-all");
        activity.onDestroy();check(takePhysical()==-1,"Destroy must release pending physical input");
        if(android.os.Build.VERSION.SDK_INT>=33){activity.commitSystemBack();check(!takeBack(),"Destroyed Activity kept its Back callback");}
        System.out.println("PASS: real GameActivity/JNI with API "+android.os.Build.VERSION.SDK_INT+" host contract");
    }
}
