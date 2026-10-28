package io.github.brew.handset;

import android.content.Context;
import android.graphics.*;
import android.graphics.drawable.*;
import android.view.*;
import android.widget.*;
import java.io.InputStream;
import java.util.*;
import java.util.function.Consumer;
import static io.github.brew.handset.ButtonBindings.Target;

/** A tela de mapeamento usa a arte e a ordem do preset Default da partida. */
final class ButtonMappingView extends FrameLayout {
    private static final Target[] TARGETS={Target.SOFT1,Target.NUM1,Target.NUM2,Target.NUM3,Target.SOFT2,
        Target.UP,Target.NUM4,Target.NUM5,Target.NUM6,Target.UP,Target.LEFT,Target.NUM7,Target.NUM8,Target.NUM9,
        Target.RIGHT,Target.DOWN,Target.STAR,Target.NUM0,Target.POUND,Target.DOWN,Target.SELECT};
    private final EnumMap<Target,ImageButton> buttons=new EnumMap<>(Target.class);
    private final EnumMap<Target,RectF> boxes=new EnumMap<>(Target.class);
    private final Map<String,Bitmap> images=new HashMap<>();
    ButtonMappingView(Context context,Consumer<Target> click){
        super(context);setBackgroundColor(Color.BLACK);
        Bitmap pressed=load("pressed-overlay-normal");
        for(int[] row:ButtonLayoutModel.DEFAULT)for(int index:row){
            Target target=TARGETS[index];Bitmap face=load(ButtonLayoutModel.ART[index]);
            StateListDrawable states=new StateListDrawable();
            states.addState(new int[]{android.R.attr.state_pressed},new LayerDrawable(new android.graphics.drawable.Drawable[]{new BitmapDrawable(getResources(),face),new BitmapDrawable(getResources(),pressed)}));
            states.addState(new int[]{},new BitmapDrawable(getResources(),face));
            ImageButton button=new ImageButton(context);button.setBackgroundColor(Color.TRANSPARENT);button.setPadding(0,0,0,0);
            button.setScaleType(ImageView.ScaleType.FIT_CENTER);button.setAlpha(1f);button.setImageAlpha(255);button.setImageDrawable(states);
            button.setFocusable(true);button.setTag("brew.mapping."+target.id);button.setContentDescription(target.label);
            button.setOnClickListener(v->click.accept(target));buttons.put(target,button);addView(button);
        }
    }
    private int dp(float value){return Math.round(value*getResources().getDisplayMetrics().density);}
    private Bitmap load(String name){
        Bitmap cached=images.get(name);if(cached!=null)return cached;
        try(InputStream input=getContext().getAssets().open("keypad/"+name+"@2x.png")){
            BitmapFactory.Options options=new BitmapFactory.Options();options.inSampleSize=2;options.inScaled=false;
            Bitmap image=BitmapFactory.decodeStream(input,null,options);if(image==null)throw new IllegalStateException("Cannot decode button artwork");
            images.put(name,image);return image;
        }catch(java.io.IOException error){throw new IllegalStateException("Could not load button artwork",error);}
    }
    void selection(Target selected,ButtonBindings bindings){
        for(Map.Entry<Target,ImageButton> entry:buttons.entrySet()){
            Target target=entry.getKey();ImageButton button=entry.getValue();
            button.setContentDescription(target.label+" · "+AndroidMappingInput.label(bindings.custom(target)));
            GradientDrawable border=new GradientDrawable();border.setColor(Color.TRANSPARENT);border.setCornerRadius(dp(5));
            if(target==selected)border.setStroke(dp(2),0xffff1f60);button.setForeground(border);
        }
    }
    @Override protected void onMeasure(int widthSpec,int heightSpec){
        int width=Math.max(1,MeasureSpec.getSize(widthSpec));
        int height=MeasureSpec.getMode(heightSpec)==MeasureSpec.UNSPECIFIED?dp(432):Math.max(1,MeasureSpec.getSize(heightSpec));
        setMeasuredDimension(width,height);boxes.clear();
        // O painel tem área própria: mantém o preset inteiro no rodapé, sem a reserva de 48% da partida.
        float scale=Math.max(0,Math.min((width-8f)/3048f,(height-8f)/2217f));
        float left=(width-3048f*scale)/2,top=height-4f-2217f*scale;
        for(int r=0;r<ButtonLayoutModel.DEFAULT.length;r++)for(int c=0;c<ButtonLayoutModel.DEFAULT[r].length;c++){
            int index=ButtonLayoutModel.DEFAULT[r][c];Target target=TARGETS[index];
            float x=left+(r==0?c*1.5f:c)*780f*scale,y=top+r*453f*scale;
            RectF rect=new RectF(x,y,x+708f*scale,y+405f*scale);boxes.put(target,rect);
            buttons.get(target).measure(MeasureSpec.makeMeasureSpec(Math.round(rect.width()),MeasureSpec.EXACTLY),MeasureSpec.makeMeasureSpec(Math.round(rect.height()),MeasureSpec.EXACTLY));
        }
    }
    @Override protected void onLayout(boolean changed,int left,int top,int right,int bottom){
        for(Map.Entry<Target,ImageButton> entry:buttons.entrySet()){
            RectF rect=boxes.get(entry.getKey());entry.getValue().layout(Math.round(rect.left),Math.round(rect.top),Math.round(rect.right),Math.round(rect.bottom));
        }
    }
}
