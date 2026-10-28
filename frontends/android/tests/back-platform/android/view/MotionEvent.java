package android.view;
public final class MotionEvent {
 public static final int ACTION_MOVE=2;
 public boolean isFromSource(int source){return false;}public int getActionMasked(){return 2;}
 public InputDevice getDevice(){return null;}public int getHistorySize(){return 0;}public int getSource(){return 0;}
 public float getAxisValue(int axis){return 0;}public float getHistoricalAxisValue(int axis,int history){return 0;}public int getDeviceId(){return 1;}
}
