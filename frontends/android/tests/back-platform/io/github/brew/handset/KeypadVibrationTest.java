package io.github.brew.handset;
public final class KeypadVibrationTest {
    public static void main(String[] args){
        GameActivity activity=new GameActivity();activity.onCreate(new android.os.Bundle());
        if(android.os.Vibrator.calls!=0)throw new AssertionError("Launching a game must not vibrate");
        activity.vibrateKeypad();if(android.os.Vibrator.calls!=1)throw new AssertionError("Keypad press did not request a vibration");
        android.os.Vibrator.present=false;activity.vibrateKeypad();if(android.os.Vibrator.calls!=1)throw new AssertionError("Missing hardware must be ignored");
        android.os.Vibrator.present=true;android.os.Vibrator.denied=true;activity.vibrateKeypad();
        if(android.os.Vibrator.calls!=1)throw new AssertionError("Denied vibration must not cause feedback");
        activity.onDestroy();System.out.println("PASS: keypad vibration request, missing hardware and permission failure");
    }
}
