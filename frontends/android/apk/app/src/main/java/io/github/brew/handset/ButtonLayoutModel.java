package io.github.brew.handset;

/** Geometria e gestos puros para testar o editor sem depender de uma tela Android. */
final class ButtonLayoutModel {
    static final int COUNT=21;
    static final String[] IDS={"soft_left","digit_1","digit_2","digit_3","soft_right","up_left","digit_4","digit_5","digit_6","up_right","left","digit_7","digit_8","digit_9","right","down_left","star","digit_0","pound","down_right","enter"};
    static final String[] LABELS={"Left soft key","1","2","3","Right soft key","Up","4","5","6","Up (legacy duplicate)","Left","7","8","9","Right","Down (legacy duplicate)","*","0","#","Down","Enter / OK"};
    static final String[] ART={"button-left-soft-key","button-1","button-2","button-3","button-left-soft-key","button-left-up","button-4","button-5","button-6","button-left-up","button-left","button-7","button-8","button-9","button-right","button-right-down","button-asterisk","button-0","button-hash","button-right-down","Dpad_Center_Rounded_Outline"};
    static final class Placement {Float x,y;float scale=1;boolean visible=true;Placement copy(){Placement p=new Placement();p.x=x;p.y=y;p.scale=scale;p.visible=visible;return p;}}
    static final class Rect {float left,top,right,bottom;Rect(float l,float t,float r,float b){left=l;top=t;right=r;bottom=b;}float width(){return right-left;}float height(){return bottom-top;}float cx(){return(left+right)/2;}float cy(){return(top+bottom)/2;}boolean contains(float x,float y){return x>=left&&x<right&&y>=top&&y<bottom;}}
    final Placement[] values=new Placement[COUNT];int selected=-1;boolean dirty;String preset="default",openingPreset;
    int pointer=-1;boolean resizing,pinching,dirtyBefore;float startX,startY,startCx,startCy,startDistance,pinchDistance,pinchScale;Placement original;
    static final int[][] DEFAULT={{0,20,4},{10,14,19,5},{18,1,2,3},{16,6,7,8},{17,11,12,13}};
    static final int[][] DPAD={{0,5,4},{10,20,14},{16,19,18}};
    static final int[][] NUMPAD={{0,20,4},{1,2,3},{6,7,8},{11,12,13},{16,17,18}};
    ButtonLayoutModel(){applyPreset("default");dirty=false;}
    int[][] rows(){return "dpad".equals(preset)?DPAD:"numpad".equals(preset)?NUMPAD:DEFAULT;}
    boolean presetVisible(int index){if(preset==null)return true;for(int[] row:rows())for(int id:row)if(id==index)return true;return false;}
    void configure(String value){cancel();preset=value;for(int i=0;i<COUNT;i++){values[i]=new Placement();values[i].visible=presetVisible(i);}selected=-1;}
    void applyPreset(String value){configure(value);dirty=hasChanges();}
    Placement[] opening;
    void markSaved(){openingPreset=preset;opening=new Placement[COUNT];for(int i=0;i<COUNT;i++)opening[i]=values[i].copy();dirty=false;}
    void reset(){applyPreset("default");}
    boolean hasChanges(){if(opening==null)return dirty;if(!java.util.Objects.equals(openingPreset,preset))return true;for(int i=0;i<COUNT;i++){Placement a=opening[i],b=values[i];if(!java.util.Objects.equals(a.x,b.x)||!java.util.Objects.equals(a.y,b.y)||Math.abs(a.scale-b.scale)>.0001f||a.visible!=b.visible)return true;}return false;}
    static float clamp(float n,float min,float max){return Math.max(min,Math.min(max,n));}
    static boolean viewport(float width,float height){return Float.isFinite(width)&&Float.isFinite(height)&&width>0&&height>0;}
    Rect rect(int index,float width,float height){
        if(!viewport(width,height)||index<0||index>=COUNT)return new Rect(0,0,0,0);
        int[][] rows=rows();float designWidth=preset==null?3828f:"default".equals(preset)?3048f:"dpad".equals(preset)?2804f:2604f;
        float designHeight="dpad".equals(preset)?1653f:"numpad".equals(preset)?2373f:2217f;
        float panel=Math.max(0,Math.min(Math.max(1,width-8)*designHeight/designWidth+8,height*.48f));
        float base=Math.min(Math.max(.01f,width-8)/designWidth,Math.max(.01f,panel-8)/designHeight);Placement p=values[index];
        float size=Math.min(base*p.scale,Math.min(width/708f,height/405f)),hx=708f*size/2,hy=405f*size/2;
        float col=index==20?2:index%5;int row=index/5;
        if(preset!=null){col=0;row=rows.length-1;for(int r=0;r<rows.length;r++)for(int c=0;c<rows[r].length;c++)if(rows[r][c]==index){row=r;col="default".equals(preset)&&r==0?c*1.5f:c;}}
        float stepX="dpad".equals(preset)?1048f:"numpad".equals(preset)?948f:780f,stepY="dpad".equals(preset)?624f:"numpad".equals(preset)?492f:453f;
        float cx=p.x==null?(width-designWidth*base)/2+(col*stepX+354f)*base:p.x*width;
        float cy=p.y==null?height-panel+4+(row*stepY+202.5f)*base:p.y*height;
        cx=clamp(cx,hx,Math.max(hx,width-hx));cy=clamp(cy,hy,Math.max(hy,height-hy));return new Rect(cx-hx,cy-hy,cx+hx,cy+hy);
    }
    void load(int index,Float x,Float y,float scale,boolean visible){Placement p=values[index];p.x=x!=null&&Float.isFinite(x)?clamp(x,0,1):null;p.y=y!=null&&Float.isFinite(y)?clamp(y,0,1):null;p.scale=Float.isFinite(scale)?clamp(scale,.25f,3):1;p.visible=visible;}
    void setScale(int index,float value){if(!Float.isFinite(value))return;value=clamp(value,.25f,3);if(Math.abs(values[index].scale-value)>.0001f){values[index].scale=value;dirty=true;}}
    void setVisible(int index,boolean value){if(values[index].visible!=value){values[index].visible=value;dirty=true;}}
    void move(int index,float x,float y,float width,float height){
        if(!viewport(width,height)||!Float.isFinite(x)||!Float.isFinite(y))return;Rect r=rect(index,width,height);x=clamp(x,r.width()/2,Math.max(r.width()/2,width-r.width()/2));y=clamp(y,r.height()/2,Math.max(r.height()/2,height-r.height()/2));
        if(Math.abs(x-r.cx())>.01f||Math.abs(y-r.cy())>.01f){values[index].x=x/width;values[index].y=y/height;dirty=true;}
    }
    int hit(float x,float y,float width,float height){for(int i=COUNT-1;i>=0;i--)if(values[i].visible&&rect(i,width,height).contains(x,y))return i;return -1;}
    boolean begin(int id,float x,float y,float width,float height,float radius){
        if(pointer!=-1||!viewport(width,height)||!Float.isFinite(x)||!Float.isFinite(y)||x<0||y<0||x>width||y>height)return false;
        Rect selectedRect=rect(selected,width,height);boolean handle=false;
        if(selected>=0&&values[selected].visible)for(float px:new float[]{selectedRect.left,selectedRect.right})for(float py:new float[]{selectedRect.top,selectedRect.bottom})handle|=Math.hypot(x-px,y-py)<=radius;
        if(!handle){int index=hit(x,y,width,height);if(index<0){selected=-1;return false;}selected=index;}
        Rect r=rect(selected,width,height);pointer=id;resizing=handle;pinching=false;original=values[selected].copy();dirtyBefore=dirty;startX=x;startY=y;startCx=r.cx();startCy=r.cy();startDistance=(float)Math.hypot(x-startCx,y-startCy);return true;
    }
    void drag(int id,float x,float y,float width,float height){
        if(id!=pointer||pointer<0||pinching||!Float.isFinite(x)||!Float.isFinite(y)||Math.hypot(x-startX,y-startY)<4)return;
        if(resizing){if(startDistance>0)setScale(selected,original.scale*(float)Math.hypot(x-startCx,y-startCy)/startDistance);}
        else move(selected,startCx+x-startX,startCy+y-startY,width,height);
    }
    void end(int id){if(id==pointer){pointer=-1;original=null;pinching=false;}}
    void cancel(){if(pointer>=0&&original!=null){values[selected]=original;dirty=dirtyBefore;}pointer=-1;original=null;pinching=false;}
    void beginPinch(float distance){if(pointer>=0&&Float.isFinite(distance)&&distance>0){pinching=true;pinchDistance=distance;pinchScale=values[selected].scale;}}
    void pinch(float distance){if(pointer>=0&&pinching&&Float.isFinite(distance)&&distance>0)setScale(selected,pinchScale*distance/pinchDistance);}
}
