package io.github.brew.handset;

public final class ButtonLayoutPresetTest {
    static void check(boolean value,String message){if(!value)throw new AssertionError(message);}
    public static void main(String[] args){
        ButtonLayoutModel m=new ButtonLayoutModel();
        check(m.selected==-1,"Editor must open without a selection");
        int visible=0;for(ButtonLayoutModel.Placement p:m.values)if(p.visible)visible++;
        check(visible==19,"New default must expose nineteen independent keys");
        m.markSaved();m.reset();check(!m.hasChanges(),"Resetting factory layout must preserve profile link");
        ButtonLayoutModel.Rect ok=m.rect(20,360,800),left=m.rect(10,360,800);
        check(ok.bottom<left.top,"OK must be in the first row");
        check(m.begin(1,left.cx(),left.cy(),360,800,12),"Visible key selects");m.end(1);
        m.begin(2,180,40,360,800,12);check(m.selected==-1,"Empty viewport tap must deselect");
        check(!m.hasChanges(),"Selection changes must not create Custom");
        String[] presets={"dpad","numpad","default"};int[] counts={9,15,19};
        for(int n=0;n<presets.length;n++){
            m.move(20,100,100,360,800);m.setScale(20,2);m.setVisible(0,false);m.applyPreset(presets[n]);
            visible=0;for(int i=0;i<ButtonLayoutModel.COUNT;i++){ButtonLayoutModel.Placement p=m.values[i];if(p.visible)visible++;check(p.x==null&&p.y==null&&p.scale==1,"Preset must replace every placement and scale");}
            check(visible==counts[n],"Preset must replace visibility");check(m.values[0].visible&&m.values[4].visible&&m.values[20].visible,"Preset restores top keys");
            if(n<2){ButtonLayoutModel.Rect a=m.rect(0,360,800),b=m.rect(n==0?5:20,360,800);check(a.width()/(b.cx()-a.cx())<.8f,"Reference presets require open horizontal spacing");}
            check(m.selected==-1&&m.pointer==-1,"Preset clears gesture and selection");
            for(float[] viewport:new float[][]{{360,800},{800,360},{240,320}}){
                for(int i=0;i<ButtonLayoutModel.COUNT;i++){ButtonLayoutModel.Rect r=m.rect(i,viewport[0],viewport[1]);check(r.left>=0&&r.top>=0&&r.right<=viewport[0]+.01f&&r.bottom<=viewport[1]+.01f,"Rotated preset inside viewport");check(Math.abs(r.width()/r.height()-708f/405)<.002f,"Preset preserves artwork aspect");}
                if(n==0){check(m.rect(5,viewport[0],viewport[1]).cy()<m.rect(20,viewport[0],viewport[1]).cy(),"D-Pad Up above OK");check(m.rect(20,viewport[0],viewport[1]).cy()<m.rect(19,viewport[0],viewport[1]).cy(),"D-Pad Down below OK");}
                if(n==1){check(m.rect(20,viewport[0],viewport[1]).cy()<m.rect(2,viewport[0],viewport[1]).cy(),"NumPad OK above digits");check(m.rect(12,viewport[0],viewport[1]).cy()<m.rect(17,viewport[0],viewport[1]).cy(),"NumPad zero below eight");}
            }
        }
        check(!m.hasChanges(),"Returning to complete original preset preserves profile link");
        m.configure(null);m.load(5,.2f,.4f,1.6f,false);m.markSaved();
        ButtonLayoutModel.Rect legacy=m.rect(20,360,800);check(legacy.top>m.rect(17,360,800).bottom,"Legacy OK remains below keypad");
        check(m.values[9].visible&&!m.values[5].visible,"Legacy duplicate visibility independent");
        check(Math.abs(m.rect(7,360,800).width()-65.10345f)<.01f,"Legacy scale baseline preserved");
        m.applyPreset("default");check(m.hasChanges(),"Reset legacy geometry is pending edit");m.configure(null);m.load(5,.2f,.4f,1.6f,false);check(!m.hasChanges(),"Restored legacy buffer retains link");
        System.out.println("PASS: presets, full replacement, no-selection, legacy and rotation behavior");
    }
}
