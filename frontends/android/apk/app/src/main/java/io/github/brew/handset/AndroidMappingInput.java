package io.github.brew.handset;

import android.view.*;

/** Consulta as faixas reais do controle, inclusive HAT e os dois padrões de eixo direito. */
final class AndroidMappingInput {
    private static final int[] AXES={0,1,11,12,13,14,15,16,17,18,22,23};
    final MappingInputState state;
    private boolean volumeKeysMappable;
    private final java.util.Set<Long> capturedVolumes=new java.util.HashSet<>();
    void setVolumeKeysMappable(boolean enabled){volumeKeysMappable=enabled;capturedVolumes.clear();state.setVolumeKeysMappable(enabled);}
    private static long volumeIdentity(KeyEvent event){return ((long)event.getDeviceId()<<32)|(event.getKeyCode()&0xffffffffL);}
    void releaseAll(){capturedVolumes.clear();state.releaseAll();}
    void releaseDevice(int device){capturedVolumes.removeIf(id->(int)(id>>32)==device);state.releaseDevice(device);}
    static boolean volume(KeyEvent event){return event.getKeyCode()==24||event.getKeyCode()==25;}
    AndroidMappingInput(MappingInputState.Sink sink){state=new MappingInputState(sink);}
    void beginCapture(){state.beginCapture(android.os.SystemClock.uptimeMillis());}
    boolean key(KeyEvent event){return key(event,false);}
    boolean key(KeyEvent event,boolean capturing){
        if(volume(event)&&(!volumeKeysMappable||(!capturing&&!capturedVolumes.contains(volumeIdentity(event)))))return false;
        boolean physicalBack=GameActivity.physicalBack(event);
        if(event.getKeyCode()==4&&!physicalBack)return false;
        if(ButtonBindings.keyToken(event.getKeyCode(),event.getScanCode(),event.getDeviceId(),volumeKeysMappable,physicalBack)==null)return false;
        if(event.getAction()!=KeyEvent.ACTION_DOWN&&event.getAction()!=KeyEvent.ACTION_UP)return false;
        if(volume(event)){if(event.getAction()==KeyEvent.ACTION_DOWN)capturedVolumes.add(volumeIdentity(event));else capturedVolumes.remove(volumeIdentity(event));}
        state.key(event.getDeviceId(),event.getKeyCode(),event.getScanCode(),event.getAction()==KeyEvent.ACTION_DOWN,event.getRepeatCount(),event.getDownTime(),physicalBack);
        return true;
    }
    boolean motion(MotionEvent event){
        if(!event.isFromSource(InputDevice.SOURCE_JOYSTICK)&&!event.isFromSource(InputDevice.SOURCE_GAMEPAD))return false;
        if(event.getActionMasked()!=MotionEvent.ACTION_MOVE)return false;
        InputDevice device=event.getDevice();if(device==null)return false;
        for(int history=0;history<=event.getHistorySize();history++)for(int axis:AXES){
            InputDevice.MotionRange range=device.getMotionRange(axis,event.getSource());if(range==null)continue;
            float value=history==event.getHistorySize()?event.getAxisValue(axis):event.getHistoricalAxisValue(axis,history);
            state.axis(event.getDeviceId(),"axis:"+axis,"axis:"+axis,value,range.getMin(),range.getMax(),range.getFlat());
        }
        return true;
    }
    static String label(String token){
        if(token==null)return "Default inputs";
        if(token.startsWith("key:")){
            String[] p=token.split(":");int code=Integer.parseInt(p[1]);
            String name=KeyEvent.keyCodeToString(code).replace("KEYCODE_","").replace('_',' ');
            return (code>=96&&code<=110?"Controller ":"Keyboard ")+name+(p.length==4?" (scan "+p[3]+")":"");
        }
        String[] p=token.split(":");int axis=Integer.parseInt(p[1]);boolean positive=p[2].equals("+");
        switch(axis){
            case 15:return "D-pad "+(positive?"right":"left");case 16:return "D-pad "+(positive?"down":"up");
            case 0:return "Left stick "+(positive?"right":"left");case 1:return "Left stick "+(positive?"down":"up");
            case 11:case 12:return "Right stick "+(positive?"right":"left");
            case 13:case 14:return "Right stick "+(positive?"down":"up");
            case 17:case 23:return "Left trigger";case 18:case 22:return "Right trigger";
            default:return "Axis "+axis+" "+p[2];
        }
    }
}
