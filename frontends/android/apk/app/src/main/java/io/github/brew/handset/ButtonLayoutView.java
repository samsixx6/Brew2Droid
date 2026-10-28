package io.github.brew.handset;

import android.content.Context;
import android.graphics.*;
import android.view.MotionEvent;
import android.view.View;
import java.io.InputStream;

/** A edição ocupa toda a área abaixo da barra; as posições salvas continuam normalizadas. */
final class ButtonLayoutView extends View {
    final ButtonLayoutModel model;final Runnable changed;final Bitmap[] faces=new Bitmap[ButtonLayoutModel.COUNT];
    final Paint paint=new Paint(Paint.ANTI_ALIAS_FLAG|Paint.FILTER_BITMAP_FLAG);final RectF preview=new RectF();
    final float density;final int alpha;final String profile;
    ButtonLayoutView(Context context,ButtonLayoutModel model,int visibility,String profile,Runnable changed){
        super(context);this.model=model;this.changed=changed;this.profile=profile;density=getResources().getDisplayMetrics().density;
        // Um botão totalmente transparente ainda precisa ser encontrável durante a edição.
        alpha=Math.max(64,Math.min(255,visibility*255/100));setBackgroundColor(Color.BLACK);setHapticFeedbackEnabled(false);setContentDescription("Touch a button to select it. Drag to move. Use handles, pinch or the size slider to resize.");
        try{for(int i=0;i<faces.length;i++){BitmapFactory.Options options=new BitmapFactory.Options();options.inSampleSize=4;options.inScaled=false;try(InputStream input=context.getAssets().open("keypad/"+ButtonLayoutModel.ART[i]+"@2x.png")){faces[i]=BitmapFactory.decodeStream(input,null,options);if(faces[i]==null)throw new IllegalStateException("Cannot decode keypad art");}}}
        catch(Exception e){release();throw new IllegalStateException("Could not load button artwork",e);}
    }
    void viewport(){
        preview.set(0,0,Math.max(1,getWidth()),Math.max(1,getHeight()));
    }
    @Override protected void onDraw(Canvas canvas){
        super.onDraw(canvas);viewport();paint.setColor(Color.BLACK);paint.setAlpha(255);canvas.drawRect(preview,paint);canvas.save();canvas.clipRect(preview);
        for(int i=0;i<faces.length;i++){if(!model.values[i].visible||faces[i]==null)continue;ButtonLayoutModel.Rect rect=model.rect(i,preview.width(),preview.height());paint.setAlpha(alpha);canvas.drawBitmap(faces[i],null,new RectF(preview.left+rect.left,preview.top+rect.top,preview.left+rect.right,preview.top+rect.bottom),paint);}
        if(model.selected>=0&&model.values[model.selected].visible){ButtonLayoutModel.Rect r=model.rect(model.selected,preview.width(),preview.height());float left=preview.left+r.left,right=preview.left+r.right,top=preview.top+r.top,bottom=preview.top+r.bottom;
            paint.setAlpha(255);paint.setColor(0xffff1f60);paint.setStyle(Paint.Style.STROKE);paint.setStrokeWidth(2*density);canvas.drawRect(left,top,right,bottom,paint);paint.setStyle(Paint.Style.FILL);
            for(float x:new float[]{left,right})for(float y:new float[]{top,bottom})canvas.drawCircle(x,y,4*density,paint);
        }
        canvas.restore();paint.setAlpha(255);if(model.selected<0){paint.setColor(0xffbdbdbd);paint.setTextSize(12*density);canvas.drawText("Profile: "+profile,12*density,20*density,paint);paint.setTextSize(14*density);paint.setTextAlign(Paint.Align.CENTER);canvas.drawText("Touch and drag a button to move it.",getWidth()/2f,44*density,paint);paint.setTextAlign(Paint.Align.LEFT);}
    }
    float distance(MotionEvent event){return (float)Math.hypot(event.getX(0)-event.getX(1),event.getY(0)-event.getY(1));}
    @Override public boolean onTouchEvent(MotionEvent event){
        viewport();int action=event.getActionMasked();boolean active=model.pointer>=0;int previousSelection=model.selected;
        if(action==MotionEvent.ACTION_DOWN){model.cancel();active=model.begin(event.getPointerId(0),event.getX()-preview.left,event.getY()-preview.top,preview.width(),preview.height(),20*density);}
        else if(action==MotionEvent.ACTION_POINTER_DOWN&&event.getPointerCount()==2)model.beginPinch(distance(event));
        else if(action==MotionEvent.ACTION_MOVE){if(model.pinching&&event.getPointerCount()>=2)model.pinch(distance(event));else{int index=event.findPointerIndex(model.pointer);if(index>=0)model.drag(model.pointer,event.getX(index)-preview.left,event.getY(index)-preview.top,preview.width(),preview.height());}}
        else if(action==MotionEvent.ACTION_UP){model.end(event.getPointerId(event.getActionIndex()));performClick();}
        else if(action==MotionEvent.ACTION_POINTER_UP){if(model.pinching||event.getPointerId(event.getActionIndex())==model.pointer)model.end(model.pointer);}
        else if(action==MotionEvent.ACTION_CANCEL)model.cancel();
        if(active||model.pointer>=0||previousSelection!=model.selected){getParent().requestDisallowInterceptTouchEvent(true);changed.run();invalidate();return true;}return super.onTouchEvent(event);
    }
    @Override public boolean performClick(){super.performClick();return true;}
    void cancelGesture(){model.cancel();invalidate();}
    void release(){for(int i=0;i<faces.length;i++)if(faces[i]!=null){faces[i].recycle();faces[i]=null;}}
}
