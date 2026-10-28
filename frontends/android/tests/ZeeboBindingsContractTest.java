package io.github.brew.handset;
import static io.github.brew.handset.ButtonBindings.Target.*;
public final class ZeeboBindingsContractTest {
 static int checks;static void check(boolean condition,String message){checks++;if(!condition)throw new AssertionError(message);}
 public static void main(String[]args){
  ButtonBindings phone=ButtonBindings.defaults().assign(NUM1,"key:96"),console=ButtonBindings.defaults(true);
  check(ButtonBindings.targets(false).length==20&&ButtonBindings.targets(true).length==19,"Active target counts");
  check(phone.target("key:96")==NUM1&&console.target("key:96")==B1,"Separate banks share a physical input");
  for(String id:new String[]{"b1","b2","b3","b4","zl","zr","home","lx-","lx+","ly-","ly+","rx-","rx+","ry-","ry+"})check(ButtonBindings.Target.fromId(id)!=null,"Missing Zeebo target: "+id);
  String[] tokens={"key:96","key:99","key:100","key:97","key:102","key:103","key:4:scan:158","axis:0:-","axis:0:+","axis:1:-","axis:1:+","axis:11:-","axis:11:+","axis:14:-","axis:14:+","axis:15:-","axis:16:+"};
  ButtonBindings.Target[] targets={B1,B2,B3,B4,ZL,ZR,HOME,LX_NEG,LX_POS,LY_NEG,LY_POS,RX_NEG,RX_POS,RY_NEG,RY_POS,LEFT,DOWN};
  for(int i=0;i<tokens.length;i++)check(console.target(tokens[i])==targets[i],"Zeebo default for "+tokens[i]);
  check(console.target("key:8")==null,"Phone number defaults cannot leak into Zeebo");
  check(console.target("axis:17:+")==ZL&&console.target("axis:23:+")==ZL&&console.target("axis:18:+")==ZR&&console.target("axis:22:+")==ZR,"Legacy trigger aliases agree with main trigger axes");
  console=console.assign(B4,"key:96").assign(LX_POS,"axis:14:+");
  check(console.target("key:96")==B4&&console.target("axis:14:+")==LX_POS,"Custom Zeebo overrides");
  check(phone.target("key:96")==NUM1,"Console edits leave phone bank intact");
  console=ButtonBindings.defaults(true);check(console.target("key:96")==B1&&phone.target("key:96")==NUM1,"Reset affects selected bank only");
  boolean refused=false;try{phone.assign(B1,"key:97");}catch(IllegalArgumentException e){refused=true;}check(refused,"Phone rejects console-only targets");
  refused=false;try{console.assign(NUM1,"key:97");}catch(IllegalArgumentException e){refused=true;}check(refused,"Console rejects phone-only targets");
  System.out.println("ZeeboBindingsContractTest: "+checks+" checks passed");
 }
}
