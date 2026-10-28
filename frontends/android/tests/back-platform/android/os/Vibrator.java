package android.os;
public final class Vibrator {
    public static boolean present=true,denied;
    public static int calls;
    public boolean hasVibrator(){return present;}
    public void vibrate(VibrationEffect effect){if(denied)throw new SecurityException();if(effect.duration<=0||effect.duration>50)throw new AssertionError("Keypad feedback must be short");calls++;}
}
