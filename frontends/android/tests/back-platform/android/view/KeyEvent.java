package android.view;
public final class KeyEvent {
 public static final int ACTION_DOWN=0,ACTION_UP=1,KEYCODE_BACK=4,FLAG_SOFT_KEYBOARD=2,FLAG_VIRTUAL_HARD_KEY=64;
 private final int action,code,scan,device,flags,repeats;private final long time;
 public KeyEvent(int action,int code,int scan,int device,int flags){this(action,code,scan,device,flags,0,100);}
 public KeyEvent(int action,int code,int scan,int device,int flags,int repeats,long time){this.action=action;this.code=code;this.scan=scan;this.device=device;this.flags=flags;this.repeats=repeats;this.time=time;}
 public int getRepeatCount(){return repeats;}public long getDownTime(){return time;}
 public static String keyCodeToString(int code){return "KEYCODE_"+code;}
 public int getAction(){return action;}public int getKeyCode(){return code;}public int getScanCode(){return scan;}public int getDeviceId(){return device;}public int getFlags(){return flags;}
 public InputDevice getDevice(){return InputDevice.getDevice(device);}
}
