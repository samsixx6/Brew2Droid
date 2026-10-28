package io.github.brew.handset;

public final class ButtonLayoutModelTest {
    static void check(boolean condition,String message){if(!condition)throw new AssertionError(message);}
    static void near(float actual,float expected){check(Math.abs(actual-expected)<0.01f,"expected "+expected+", got "+actual);}
    static void defaults(){
        ButtonLayoutModel m=new ButtonLayoutModel();ButtonLayoutModel.Rect one=m.rect(1,360,800),enter=m.rect(20,360,800);
        check(one.width()>40,"key face missing");near(enter.width(),one.width());near(enter.cx(),180);check(enter.bottom<m.rect(10,360,800).top,"OK above directions");check(!m.dirty,"initial layout is clean");
    }
    static void touchSelect(){
        ButtonLayoutModel m=new ButtonLayoutModel();ButtonLayoutModel.Rect r=m.rect(1,360,800);
        check(m.begin(7,r.cx(),r.cy(),360,800,12),"touch begins");check(m.selected==1,"touch selects key");m.drag(99,r.cx()+50,r.cy()-100,360,800);m.end(7);
        near(m.rect(1,360,800).cx(),r.cx());check(!m.dirty,"selection and foreign pointer do not edit");
    }
    static void drag(){
        ButtonLayoutModel m=new ButtonLayoutModel();ButtonLayoutModel.Rect r=m.rect(7,360,800);check(m.begin(3,r.cx(),r.cy(),360,800,12),"touch begins");m.drag(3,r.cx()-30,r.cy()-200,360,800);m.end(3);
        near(m.rect(7,360,800).cx(),r.cx()-30);near(m.rect(7,360,800).cy(),r.cy()-200);check(m.dirty,"drag persists");near(m.values[7].x,(r.cx()-30)/360);near(m.values[7].y,(r.cy()-200)/800);
    }
    static void cancel(){
        ButtonLayoutModel m=new ButtonLayoutModel();ButtonLayoutModel.Rect r=m.rect(7,360,800);check(m.begin(1,r.cx(),r.cy(),360,800,12),"touch begins");m.drag(1,r.cx()+35,r.cy()-100,360,800);m.cancel();
        near(m.rect(7,360,800).cx(),r.cx());near(m.rect(7,360,800).cy(),r.cy());check(!m.dirty,"cancel restores clean state");
        m.setVisible(0,false);check(m.begin(1,r.cx(),r.cy(),360,800,12),"new touch begins");m.drag(1,r.cx()+20,r.cy()-80,360,800);m.cancel();check(m.dirty&&!m.values[0].visible,"cancel preserves earlier edits");
    }
    static void resize(){
        ButtonLayoutModel m=new ButtonLayoutModel();ButtonLayoutModel.Rect r=m.rect(7,360,800);m.selected=7;check(m.begin(4,r.right,r.bottom,360,800,12),"handle begins");m.drag(4,r.cx()+r.width()*.75f,r.cy()+r.height()*.75f,360,800);m.end(4);
        near(m.values[7].scale,1.5f);near(m.rect(7,360,800).width()/m.rect(7,360,800).height(),708f/405);check(m.dirty,"handle resizes");
    }
    static void pinch(){
        ButtonLayoutModel m=new ButtonLayoutModel();ButtonLayoutModel.Rect r=m.rect(7,360,800);check(m.begin(4,r.cx(),r.cy(),360,800,12),"touch begins");m.beginPinch(40);m.pinch(80);m.end(4);near(m.values[7].scale,2);check(m.dirty,"pinch resizes");
        m.setScale(7,Float.NaN);near(m.values[7].scale,2);
    }
    static void hide(){
        ButtonLayoutModel m=new ButtonLayoutModel();m.configure(null);ButtonLayoutModel.Rect r=m.rect(5,360,800);m.setVisible(5,false);check(!m.values[5].visible&&m.values[9].visible,"duplicated Up buttons are independent");check(m.hit(r.cx(),r.cy(),360,800)==-1,"hidden key cannot be selected on canvas");m.setVisible(5,true);check(m.hit(r.cx(),r.cy(),360,800)==5,"checked key returns");
    }
    static void bounds(){
        for(float[] size:new float[][]{{240,320},{800,360},{360,800}}){ButtonLayoutModel m=new ButtonLayoutModel();m.setScale(20,90);near(m.values[20].scale,3);m.move(20,-100,9999,size[0],size[1]);ButtonLayoutModel.Rect r=m.rect(20,size[0],size[1]);check(r.left>=0&&r.top>=0&&r.right<=size[0]+.01f&&r.bottom<=size[1]+.01f,"clamped");near(r.width()/r.height(),708f/405);m.setScale(20,0);near(m.values[20].scale,.25f);}
    }
    static void overlap(){
        ButtonLayoutModel m=new ButtonLayoutModel();ButtonLayoutModel.Rect r=m.rect(7,360,800);m.move(12,r.cx(),r.cy(),360,800);check(m.hit(r.cx(),r.cy(),360,800)==12,"topmost key is selectable");m.setVisible(12,false);check(m.hit(r.cx(),r.cy(),360,800)==7,"underlying key returns");
    }
    static void load(){
        ButtonLayoutModel m=new ButtonLayoutModel();m.load(7,.4f,.2f,1.5f,false);check(!m.dirty,"loading is not an edit");check(!m.values[7].visible,"visibility loaded");near(m.rect(7,360,800).cx(),144);near(m.rect(7,360,800).cy(),160);
        check(!m.begin(9,-5,-5,360,800,12),"outside touch ignored");check(!m.dirty,"outside touch is not edit");
    }
    public static void main(String[] args){String[] names={"defaults","touch selection","drag","cancel","resize","pinch","hide","bounds","overlap","load"};Runnable[] tests={ButtonLayoutModelTest::defaults,ButtonLayoutModelTest::touchSelect,ButtonLayoutModelTest::drag,ButtonLayoutModelTest::cancel,ButtonLayoutModelTest::resize,ButtonLayoutModelTest::pinch,ButtonLayoutModelTest::hide,ButtonLayoutModelTest::bounds,ButtonLayoutModelTest::overlap,ButtonLayoutModelTest::load};int failed=0;for(int i=0;i<tests.length;i++){try{tests[i].run();System.out.println("PASS: "+names[i]);}catch(Throwable e){failed++;System.err.println("FAIL: "+names[i]+": "+e);}}if(failed>0)throw new AssertionError(failed+" touch editor cases failed");System.out.println("PASS: 10 touch layout cases");}
}
