package io.github.brew.handset;
import java.util.*;

/** Adaptado do Pocket2Droid: captura só aceita uma pressão nova após o bloqueio de foco. */
final class MappingInputState {
 interface Sink {void input(int deviceId,String token,boolean down,boolean fresh);}
 private static final class Report {boolean down,blocked;long pressTime=Long.MIN_VALUE;String token;Report(String t){token=t;}}
 private static final class Axis {boolean armed,neutral;}
 private static final class Device {final Map<String,Report> reports=new LinkedHashMap<>();final Map<String,Axis> axes=new HashMap<>();}
 private boolean volumeKeysMappable;
 void setVolumeKeysMappable(boolean enabled){if(volumeKeysMappable!=enabled){releaseAll();volumeKeysMappable=enabled;}}
 private final Map<Integer,Device> devices=new LinkedHashMap<>();private final Sink sink;private long gateTime=Long.MIN_VALUE;
 MappingInputState(Sink sink){this.sink=sink;}
 private Device device(int id){Device d=devices.get(id);if(d==null){d=new Device();devices.put(id,d);}return d;}
 private void report(int id,String channel,String token,boolean down,boolean fresh){Device d=device(id);Report r=d.reports.get(channel);if(r==null){r=new Report(token);d.reports.put(channel,r);}if(r.down==down)return;
  boolean before=held(d,token);r.down=down;r.blocked=false;boolean after=held(d,token);if(before!=after)sink.input(id,token,after,after&&fresh);
 }
 private boolean held(Device d,String token){for(Report r:d.reports.values())if(r.down&&r.token.equals(token))return true;return false;}
 void key(int id,int key,boolean down,int repeats){key(id,key,down,repeats,Long.MIN_VALUE);}
 void key(int id,int key,boolean down,int repeats,long pressTime){
  key(id,key,0,down,repeats,pressTime);
 }
 void key(int id,int key,int scan,boolean down,int repeats,long pressTime){
  key(id,key,scan,down,repeats,pressTime,false);
 }
 void key(int id,int key,int scan,boolean down,int repeats,long pressTime,boolean physicalBack){
  String token=ButtonBindings.keyToken(key,scan,id,volumeKeysMappable,physicalBack);if(token==null)return;
  String channel=token;Device d=device(id);Report old=d.reports.get(channel);
  if(old!=null&&pressTime!=Long.MIN_VALUE&&pressTime<old.pressTime)return;
  boolean newIdentity=old!=null&&old.pressTime!=Long.MIN_VALUE&&pressTime>old.pressTime;
  boolean fresh=down&&repeats==0&&(old==null||!old.down||newIdentity)&&pressTime>=gateTime;
  if(fresh){
   // A soltura anterior pode ter ido à janela do diálogo; a próxima pressão a substitui.
   boolean before=held(d,token);
   for(Report r:d.reports.values())if(r.token.equals(token)&&(newIdentity||r.blocked)){r.down=false;r.blocked=false;}
   if(before&&!held(d,token))sink.input(id,token,false,false);
  }
  report(id,channel,token,down,fresh);
  if(down)d.reports.get(channel).pressTime=pressTime;
 }
 void axis(int id,String channel,String control,float value,float min,float max,float flat){
  if(!Float.isFinite(value)||!Float.isFinite(min)||!Float.isFinite(max)||!Float.isFinite(flat)||max<=min)return;
  Device d=device(id);String axisId=channel+":"+control;Axis a=d.axes.get(axisId);if(a==null){a=new Axis();d.axes.put(axisId,a);}
  boolean trigger=control.matches("axis:(17|18|22|23)"),hat=control.equals("axis:15")||control.equals("axis:16");
  float center=trigger?min:(min<0&&max>0?0:(min+max)/2);float delta=Math.max(min,Math.min(max,value))-center;
  float usable=delta>=0?max-center:center-min;float dead=Math.max(0,Math.min(flat,usable));
  float magnitude=usable>dead?Math.max(0,(Math.abs(delta)-dead)/(usable-dead)):0;
  a.neutral=hat?delta==0:magnitude<.35f;if(a.neutral)a.armed=true;
  if(trigger){transition(id,axisId,control+":+",magnitude,a.armed,false);}
  else {
   String negative=control+":-",positive=control+":+";
   // A direção velha solta antes da pressão oposta.
   if(delta>=0){transition(id,axisId+"-",negative,0,a.armed,hat);transition(id,axisId+"+",positive,magnitude,a.armed,hat);}
   else{transition(id,axisId+"+",positive,0,a.armed,hat);transition(id,axisId+"-",negative,magnitude,a.armed,hat);}
  }
 }
 private boolean heldChannel(Device d,String channel){Report r=d.reports.get(channel);return r!=null&&r.down;}
 private void transition(int id,String channel,String token,float magnitude,boolean fresh,boolean hat){boolean was=heldChannel(device(id),channel);boolean down=hat?magnitude>0:was?magnitude>=.35f:magnitude>=.65f;report(id,channel,token,down,fresh);}
 void beginCapture(){beginCapture(gateTime);}
 void beginCapture(long time){gateTime=Math.max(gateTime,time);for(Device d:devices.values()){for(Axis a:d.axes.values())a.armed=a.neutral;for(Report r:d.reports.values())if(r.down)r.blocked=true;}}
 void releaseDevice(int id){Device d=devices.remove(id);if(d==null)return;Set<String> released=new LinkedHashSet<>();for(Report r:d.reports.values())if(r.down)released.add(r.token);for(String t:released)sink.input(id,t,false,false);}
 void releaseAll(){for(int id:new ArrayList<>(devices.keySet()))releaseDevice(id);}
}
