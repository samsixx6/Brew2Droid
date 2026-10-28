package io.github.brew.handset;

import android.app.NativeActivity;
import android.hardware.input.InputManager;
import android.os.*;
import android.view.KeyEvent;
import android.view.InputDevice;

/** O NDK não avisa quando o controle some sem KeyUp; o callback só enfileira a soltura. */
public final class GameActivity extends NativeActivity implements InputManager.InputDeviceListener {
    static {System.loadLibrary("zeebx_android");}
    private InputManager manager;
    private Object backCallback;

    private Vibrator vibrator;
    private InGameMenu gameMenu;
    private static native void nativeMenuAction(int action);
    public void showGameMenu(boolean counter,boolean keypad){if(gameMenu==null)gameMenu=new InGameMenu(this,GameActivity::nativeMenuAction);gameMenu.show(counter,keypad);}
    public void dismissGameMenu(){if(gameMenu!=null)gameMenu.dismiss();}
    public void vibrateKeypad(){
        try{if(vibrator==null)vibrator=getSystemService(Vibrator.class);
            if(vibrator!=null&&vibrator.hasVibrator())vibrator.vibrate(VibrationEffect.createOneShot(18,VibrationEffect.DEFAULT_AMPLITUDE));
        }catch(SecurityException ignored){}
    }
    private static native void nativeInputDeviceRemoved(int deviceId);
    private static native void nativeRequestBack();
    private static native void nativePhysicalBack(int device,int scan,boolean down);
    private static native void nativeReleasePhysicalBack();
    static boolean physicalBack(KeyEvent event){
        if(event.getKeyCode()!=KeyEvent.KEYCODE_BACK||event.getDeviceId()<0)return false;
        if((event.getFlags()&(KeyEvent.FLAG_VIRTUAL_HARD_KEY|KeyEvent.FLAG_SOFT_KEYBOARD))!=0)return false;
        InputDevice device=event.getDevice();
        return event.getScanCode()>=0&&(device==null?event.getScanCode()>0:!device.isVirtual());
    }
    @Override public boolean dispatchKeyEvent(KeyEvent event){
        if(physicalBack(event)&&(event.getAction()==KeyEvent.ACTION_DOWN||event.getAction()==KeyEvent.ACTION_UP)){
            nativePhysicalBack(event.getDeviceId(),event.getScanCode(),event.getAction()==KeyEvent.ACTION_DOWN);
            return true;
        }
        return super.dispatchKeyEvent(event);
    }
    @Override protected void onPause(){nativeReleasePhysicalBack();super.onPause();}
    @Override public void onCreate(Bundle state){
        super.onCreate(state);
        nativeReleasePhysicalBack();
        manager=(InputManager)getSystemService(INPUT_SERVICE);
        manager.registerInputDeviceListener(this,new Handler(Looper.getMainLooper()));
        if(Build.VERSION.SDK_INT>=33)backCallback=BackApi33.register(this);
    }
    @Override public void onBackPressed(){nativeRequestBack();}
    // Isolar os tipos da API 33 evita carregá-los nos aparelhos Android antigos.
    private static final class BackApi33 {
        static Object register(GameActivity activity){
            android.window.OnBackInvokedCallback callback=activity::onBackPressed;
            activity.getOnBackInvokedDispatcher().registerOnBackInvokedCallback(android.window.OnBackInvokedDispatcher.PRIORITY_DEFAULT,callback);
            return callback;
        }
        static void unregister(GameActivity activity,Object callback){activity.getOnBackInvokedDispatcher().unregisterOnBackInvokedCallback((android.window.OnBackInvokedCallback)callback);}
    }
    public void onInputDeviceAdded(int id){}
    public void onInputDeviceChanged(int id){nativeInputDeviceRemoved(id);}
    public void onInputDeviceRemoved(int id){nativeInputDeviceRemoved(id);}
    @Override protected void onDestroy(){nativeReleasePhysicalBack();if(gameMenu!=null)gameMenu.dispose();if(backCallback!=null)BackApi33.unregister(this,backCallback);if(manager!=null)manager.unregisterInputDeviceListener(this);super.onDestroy();}
}
