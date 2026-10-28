package android.app;
import android.window.*;

/** Fronteira mínima do framework: Android 16 não entrega Back como KeyEvent/onBackPressed. */
public class NativeActivity {
    public static final String INPUT_SERVICE="input";
    private boolean finishing;
    private OnBackInvokedCallback callback;
    private final OnBackInvokedDispatcher dispatcher=new OnBackInvokedDispatcher(){
        public void registerOnBackInvokedCallback(int priority,OnBackInvokedCallback value){if(priority!=0)throw new AssertionError("Back callback must consume navigation");callback=value;}
        public void unregisterOnBackInvokedCallback(OnBackInvokedCallback value){if(callback==value)callback=null;}
    };
    public void onCreate(android.os.Bundle state){}
    protected void onDestroy(){}
    protected void onPause(){}
    public boolean dispatchKeyEvent(android.view.KeyEvent event){if(event.getKeyCode()==4&&event.getAction()==1)onBackPressed();return true;}
    public Object getSystemService(String name){return new android.hardware.input.InputManager();}
    public <T>T getSystemService(Class<T> type){return type.cast(new android.os.Vibrator());}
    public OnBackInvokedDispatcher getOnBackInvokedDispatcher(){return dispatcher;}
    public void onBackPressed(){finishing=true;}
    public boolean isFinishing(){return finishing;}
    public void commitSystemBack(){
        if(android.os.Build.VERSION.SDK_INT>=33&&callback!=null)callback.onBackInvoked();
        else if(android.os.Build.VERSION.SDK_INT>=33)finishing=true;
        else onBackPressed();
    }
}
