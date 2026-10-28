package io.github.brew.handset;
import static io.github.brew.handset.ButtonBindings.Target.*;

public final class ButtonBindingsTest {
    static int checks;
    static void check(boolean value,String message){checks++;if(!value)throw new AssertionError(message);}
    public static void main(String[] args){
        ButtonBindings b=ButtonBindings.defaults();
        check(b.target("key:96")==SELECT,"Default controller A must stay OK");
        check(b.target("key:82")==SOFT1,"DOOV Menu must stay left soft key");
        check(b.target("key:4:scan:158")==SOFT2,"Physical DOOV Back must stay right soft key");
        check(b.target("key:4")==null,"Android Back is navigation");
        check(b.target("key:24")==null,"Volume is media by default");
        check(ButtonBindings.keyToken(4,0,1)==null,"Back without physical scan cannot be captured");
        check(ButtonBindings.keyToken(4,158,-1)==null,"Virtual Back cannot be captured");
        check("key:4:scan:158".equals(ButtonBindings.keyToken(4,158,2)),"Physical scan must survive");
        check(!ButtonBindings.validToken("key:3")&&!ButtonBindings.validToken("key:26"),"Home/power reserved");
        check(ButtonBindings.validToken("key:24")&&ButtonBindings.validToken("key:25")&&!ButtonBindings.validToken("key:164"),"Stored volume up/down bindings stay valid while mute remains reserved");
        check(ButtonBindings.validToken("axis:0:-")&&!ButtonBindings.validToken("axis:100:+"),"Native axes validated");
        b=b.assign(NUM1,"key:96");check(b.target("key:96")==NUM1,"Capture must override original input route");
        b=b.assign(NUM2,"key:96");check(b.custom(NUM1)==null&&b.target("key:96")==NUM2,"Assignment displaces duplicate");
        b=b.assign(CLEAR,"key:29");check(b.target("key:29")==CLEAR,"Keyboard assignment accepted");
        b=ButtonBindings.defaults();check(b.target("key:96")==SELECT&&b.target("key:24")==null,"Reset restores defaults");
        System.out.println("ButtonBindingsTest: "+checks+" checks passed");
    }
}
