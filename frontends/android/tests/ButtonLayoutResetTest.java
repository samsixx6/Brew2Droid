package io.github.brew.handset;

public final class ButtonLayoutResetTest {
    static void check(boolean ok,String message){if(!ok)throw new AssertionError(message);}
    public static void main(String[] args){
        ButtonLayoutModel m=new ButtonLayoutModel();m.markSaved();m.reset();check(!m.hasChanges(),"Resetting the default layout must not create Custom");
        m.load(7,.2f,.3f,2,false);m.markSaved();m.reset();check(m.hasChanges(),"Reset of saved customization must be pending until Save");
        for(int i=0;i<ButtonLayoutModel.COUNT;i++){ButtonLayoutModel.Placement p=m.values[i];check(p.x==null&&p.y==null&&p.scale==1&&p.visible==m.presetVisible(i),"All button placements, sizes and visibility must return to defaults");}
        ButtonLayoutModel expected=new ButtonLayoutModel();for(float[] viewport:new float[][]{{360,800},{800,360}}){ButtonLayoutModel.Rect actual=m.rect(7,viewport[0],viewport[1]),want=expected.rect(7,viewport[0],viewport[1]);check(actual.left==want.left&&actual.top==want.top&&actual.right==want.right&&actual.bottom==want.bottom,"Reset must restore adaptive layout in either orientation");}
        m.markSaved();check(!m.hasChanges(),"Saved reset is clean");
        ButtonLayoutModel.Rect r=m.rect(7,360,800);m.begin(1,r.cx(),r.cy(),360,800,12);m.drag(1,200,300,360,800);m.reset();check(m.pointer==-1&&!m.hasChanges(),"Reset must end active drag without keeping moved state");
        System.out.println("PASS: 27 editor reset cases");
    }
}
