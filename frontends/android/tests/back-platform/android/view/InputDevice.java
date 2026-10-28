package android.view;
public final class InputDevice {
 public static final int SOURCE_JOYSTICK=0x1000010,SOURCE_GAMEPAD=0x401;
 public static final class MotionRange {public float getMin(){return -1;}public float getMax(){return 1;}public float getFlat(){return 0;}}
 public MotionRange getMotionRange(int axis,int source){return null;}
 private final boolean virtual;public InputDevice(boolean virtual){this.virtual=virtual;}
 public boolean isVirtual(){return virtual;}
 public static InputDevice getDevice(int id){return id<0?null:new InputDevice(id==99);}
}
