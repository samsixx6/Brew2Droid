package io.github.brew.handset;
import java.util.*;

/** Contrato de entrada puro, exercitado também no host sem a camada Android. */
final class ButtonBindings {
    enum Target {
        NUM0("0","0"),NUM1("1","1"),NUM2("2","2"),NUM3("3","3"),NUM4("4","4"),
        NUM5("5","5"),NUM6("6","6"),NUM7("7","7"),NUM8("8","8"),NUM9("9","9"),
        STAR("*","*"),POUND("#","#"),UP("up","Up"),DOWN("down","Down"),LEFT("left","Left"),
        RIGHT("right","Right"),SELECT("select","OK"),SOFT1("soft1","Left soft key"),
        SOFT2("soft2","Right soft key"),CLEAR("clear","Clear"),
        B1("b1","Button 1"),B2("b2","Button 2"),B3("b3","Button 3"),B4("b4","Button 4"),
        ZL("zl","ZL"),ZR("zr","ZR"),HOME("home","HOME"),
        LX_NEG("lx-","Left stick · Left"),LX_POS("lx+","Left stick · Right"),
        LY_NEG("ly-","Left stick · Up"),LY_POS("ly+","Left stick · Down"),
        RX_NEG("rx-","Right stick · Left"),RX_POS("rx+","Right stick · Right"),
        RY_NEG("ry-","Right stick · Up"),RY_POS("ry+","Right stick · Down");
        final String id,label;Target(String id,String label){this.id=id;this.label=label;}
        static Target fromId(String id){for(Target t:values())if(t.id.equals(id))return t;return null;}
    }
    private final EnumMap<Target,String> custom;
    private final boolean zeebo;
    private ButtonBindings(EnumMap<Target,String> values,boolean zeebo){custom=new EnumMap<>(values);this.zeebo=zeebo;}
    boolean isZeebo(){return zeebo;}
    static Target[] targets(boolean zeebo){return zeebo?new Target[]{Target.B1,Target.B2,Target.B3,Target.B4,Target.ZL,Target.ZR,Target.HOME,Target.UP,Target.DOWN,Target.LEFT,Target.RIGHT,Target.LX_NEG,Target.LX_POS,Target.LY_NEG,Target.LY_POS,Target.RX_NEG,Target.RX_POS,Target.RY_NEG,Target.RY_POS}:Arrays.copyOf(Target.values(),20);}
    static ButtonBindings defaults(){return defaults(false);}
    static ButtonBindings defaults(boolean zeebo){return new ButtonBindings(new EnumMap<>(Target.class),zeebo);}
    String custom(Target target){return custom.get(target);}
    Target target(String token){
        if(!validToken(token))return null;
        for(Map.Entry<Target,String> e:custom.entrySet())if(e.getValue().equals(token))return e.getKey();
        if(zeebo)return zeeboDefault(token);
        if(token.startsWith("key:4:scan:"))return Target.SOFT2;
        switch(token){
            case "axis:15:-":return Target.LEFT;case "axis:15:+":return Target.RIGHT;
            case "axis:16:-":return Target.UP;case "axis:16:+":return Target.DOWN;
        }
        if(!token.startsWith("key:"))return null;
        int code=Integer.parseInt(token.substring(4));
        if(code>=7&&code<=16)return Target.values()[code-7];
        switch(code){
            case 17:return Target.STAR;case 18:return Target.POUND;case 19:return Target.UP;
            case 20:return Target.DOWN;case 21:return Target.LEFT;case 22:return Target.RIGHT;
            case 23:case 66:case 96:return Target.SELECT;
            case 1:case 102:case 82:return Target.SOFT1;case 2:case 103:return Target.SOFT2;
            case 28:case 67:return Target.CLEAR;default:return null;
        }
    }
    ButtonBindings assign(Target target,String token){
        if(target==null||!Arrays.asList(targets(zeebo)).contains(target)||!validToken(token))throw new IllegalArgumentException("Invalid input assignment");
        EnumMap<Target,String> next=new EnumMap<>(custom);next.values().remove(token);next.put(target,token);
        return new ButtonBindings(next,zeebo);
    }
    private static Target zeeboDefault(String token){
        if(token.startsWith("key:4:scan:"))return Target.HOME;
        switch(token){
            case "axis:0:-":return Target.LX_NEG;case "axis:0:+":return Target.LX_POS;
            case "axis:1:-":return Target.LY_NEG;case "axis:1:+":return Target.LY_POS;
            case "axis:11:-":return Target.RX_NEG;case "axis:11:+":return Target.RX_POS;
            case "axis:14:-":return Target.RY_NEG;case "axis:14:+":return Target.RY_POS;
            case "axis:15:-":return Target.LEFT;case "axis:15:+":return Target.RIGHT;
            case "axis:16:-":return Target.UP;case "axis:16:+":return Target.DOWN;
            case "axis:17:-":case "axis:17:+":case "axis:23:-":case "axis:23:+":return Target.ZL;
            case "axis:18:-":case "axis:18:+":case "axis:22:-":case "axis:22:+":return Target.ZR;
        }
        if(!token.startsWith("key:"))return null;
        switch(Integer.parseInt(token.substring(4))){
            case 96:case 66:case 62:case 54:return Target.B1;
            case 99:case 52:return Target.B2;case 100:case 29:return Target.B3;case 97:case 47:return Target.B4;
            case 102:return Target.ZL;case 103:return Target.ZR;
            case 108:case 109:case 110:case 111:return Target.HOME;
            case 19:return Target.UP;case 20:return Target.DOWN;case 21:return Target.LEFT;case 22:return Target.RIGHT;
            default:return null;
        }
    }
    static boolean validToken(String token){
        if(token==null)return false;
        try{
            if(token.matches("key:4:scan:(0|[1-9][0-9]{0,9})"))return Integer.parseInt(token.substring(11))>=0;
            if(token.matches("key:[1-9][0-9]{0,3}")){int code=Integer.parseInt(token.substring(4));return code<=1023&&code!=3&&code!=4&&code!=26&&code!=164&&code!=187&&code!=223&&code!=224;}
            return token.matches("axis:(0|1|11|12|13|14|15|16|17|18|22|23):[+-]");
        }catch(NumberFormatException e){return false;}
    }
    static String keyToken(int code,int scan,int device){return keyToken(code,scan,device,false,false);}
    static String keyToken(int code,int scan,int device,boolean volumeKeysMappable,boolean physicalBack){
        if((code==24||code==25)&&!volumeKeysMappable)return null;
        String token=code==4?((scan>0||physicalBack)&&scan>=0&&device>=0?"key:4:scan:"+scan:null):"key:"+code;
        return validToken(token)?token:null;
    }
}
