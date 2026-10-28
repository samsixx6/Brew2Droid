package io.github.brew.handset;

public final class ButtonLayoutRevertTest {
    static void check(boolean ok,String message){if(!ok)throw new AssertionError(message);}
    public static void main(String[] args){
        ButtonLayoutModel m=new ButtonLayoutModel();m.markSaved();
        m.setVisible(5,false);check(m.hasChanges(),"Hiding must count as edit");
        m.setVisible(5,true);check(!m.hasChanges(),"Restored visibility must preserve live profile link");
        m.setScale(7,1.2f);check(m.hasChanges(),"Resize must count as edit");
        m.setScale(7,1);check(!m.hasChanges(),"Restored size must not create Custom");
        m.load(7,.4f,.2f,1.5f,false);m.markSaved();check(!m.hasChanges(),"Loaded profile is the baseline");
        m.move(7,220,250,360,800);check(m.hasChanges(),"Position differs from loaded baseline");
        m.load(7,.4f,.2f,1.5f,false);check(!m.hasChanges(),"Restored buffer must match opening profile");
        System.out.println("PASS: 7 unchanged layout save cases");
    }
}
