package android.hardware.input;
public final class InputManager {
    public interface InputDeviceListener {void onInputDeviceAdded(int id);void onInputDeviceChanged(int id);void onInputDeviceRemoved(int id);}
    public void registerInputDeviceListener(InputDeviceListener listener,android.os.Handler handler){}
    public void unregisterInputDeviceListener(InputDeviceListener listener){}
}
