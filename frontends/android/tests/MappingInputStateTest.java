package io.github.brew.handset;
import java.util.*;

public final class MappingInputStateTest {
    static void check(boolean value,String message){if(!value)throw new AssertionError(message);}
    public static void main(String[] args){
        List<String> events=new ArrayList<>();
        MappingInputState input=new MappingInputState((device,token,down,fresh)->events.add(device+" "+token+" "+down+" "+fresh));
        input.key(2,19,true,0,100);
        check(events.equals(Arrays.asList("2 key:19 true true")),"Android native key token must reach capture");
        input.beginCapture(200);events.clear();input.key(2,19,true,1,100);
        check(events.isEmpty(),"Held/repeating key cannot become a new capture");
        input.key(2,19,false,0,100);input.key(2,19,true,0,300);
        check(events.get(events.size()-1).equals("2 key:19 true true"),"Next physical press is fresh");
        input.axis(3,"axis:0","axis:0",.9f,-1,1,0);
        check(events.get(events.size()-1).equals("3 axis:0:+ true false"),"Initially held stick cannot capture");
        input.axis(3,"axis:0","axis:0",0,-1,1,0);input.axis(3,"axis:0","axis:0",-.9f,-1,1,0);
        check(events.get(events.size()-1).equals("3 axis:0:- true true"),"Stick captures only after neutral");
        input.axis(3,"axis:0","axis:0",-.5f,-1,1,0);int count=events.size();
        input.axis(3,"axis:0","axis:0",Float.NaN,-1,1,0);check(events.size()==count,"Invalid axis report ignored");
        input.releaseDevice(3);check(events.get(events.size()-1).equals("3 axis:0:- false false"),"Disconnect releases axes");
        input.releaseAll();check(events.get(events.size()-1).equals("2 key:19 false false"),"Focus loss releases held keys");
        events.clear();input.key(1,24,0,true,0,400);check(events.isEmpty(),"Volume stays Android without opt-in");
        input.setVolumeKeysMappable(true);input.beginCapture(400);input.key(1,24,0,true,0,500);
        check(events.equals(Arrays.asList("1 key:24 true true")),"Opted-in Volume Up captures once");
        input.key(1,24,0,true,6,500);check(events.size()==1,"Volume repeat cannot capture twice");
        input.key(1,24,0,false,0,500);check(events.get(1).equals("1 key:24 false false"),"Volume release delivered");
        input.key(1,25,0,true,0,600);check(events.get(2).equals("1 key:25 true true"),"Volume Down can map any target");
        input.setVolumeKeysMappable(false);check(events.get(3).equals("1 key:25 false false"),"Disabling opt-in releases held volume");
        events.clear();input.key(1,4,0,true,0,700,true);check(events.equals(Arrays.asList("1 key:4:scan:0 true true")),"Host-confirmed OEM Back without scan captures");
        System.out.println("MappingInputStateTest: 15 checks passed");
    }
}
