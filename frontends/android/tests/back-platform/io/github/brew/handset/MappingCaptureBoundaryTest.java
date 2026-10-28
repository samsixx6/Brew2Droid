package io.github.brew.handset;
import android.view.KeyEvent;
import java.util.*;

/** Executa o adaptador Android real; o framework de host apenas fornece KeyEvents. */
public final class MappingCaptureBoundaryTest {
 private static void check(boolean ok,String message){if(!ok)throw new AssertionError(message);}
 private static KeyEvent key(int action,int code,int repeats,long time){return new KeyEvent(action,code,0,1,0,repeats,time);}
 public static void main(String[] args){
  List<String> events=new ArrayList<>();AndroidMappingInput input=new AndroidMappingInput((device,token,down,fresh)->events.add(token+" "+down+" "+fresh));
  check(!input.key(key(0,24,0,100),true),"Default capture must leave volume to Android");
  input.setVolumeKeysMappable(true);input.beginCapture();
  check(!input.key(key(0,24,0,100)),"Volume outside capture must stay Android");
  check(input.key(key(0,24,0,200),true),"Enabled capture must consume volume down");
  check(events.equals(Arrays.asList("key:24 true true")),"Volume down did not capture once");
  check(input.key(key(0,24,3,200)),"Repeats after dialog dismissal must remain consumed");
  check(events.size()==1,"Repeats generated duplicate capture");
  check(input.key(key(1,24,0,200)),"Release after dialog dismissal must remain consumed");
  check(events.get(1).equals("key:24 false false"),"Volume release lost");
  check(!input.key(key(0,24,0,300)),"New volume press outside capture must return to Android");
  check(input.key(key(0,25,0,400),true),"Volume Down capture failed");
  input.releaseAll();
  check(!input.key(key(0,25,0,500)),"Focus loss left a stale volume interception outside capture");
  check(input.key(key(0,25,0,600),true),"Capture after resume failed");input.releaseDevice(1);
  check(!input.key(key(0,25,0,700)),"Device removal left stale capture interception");
  check(input.key(new KeyEvent(0,4,0,1,0),true),"Physical OEM Back with scan zero failed capture");
  check(!input.key(new KeyEvent(0,4,158,-1,64),true),"Virtual Back entered mapping capture");
  System.out.println("PASS: real AndroidMappingInput capture, repeats, dismissal release and volume fallback");
 }
}
