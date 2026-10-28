package android.os;
public final class VibrationEffect {
    public static final int DEFAULT_AMPLITUDE=-1;
    public final long duration;
    private VibrationEffect(long duration){this.duration=duration;}
    public static VibrationEffect createOneShot(long duration,int amplitude){return new VibrationEffect(duration);}
}
