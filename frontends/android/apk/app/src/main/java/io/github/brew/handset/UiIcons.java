package io.github.brew.handset;

import android.graphics.*;
import android.graphics.drawable.Drawable;

/** Ícones vetoriais dos menus; não carregam imagens nem participam da tela do jogo. */
final class UiIcons extends Drawable {
    final String kind;
    final int size;
    final Paint pen=new Paint(Paint.ANTI_ALIAS_FLAG);
    UiIcons(String kind,int color,int size){this.kind=kind;this.size=size;pen.setColor(color);pen.setStrokeWidth(1.8f);pen.setStrokeCap(Paint.Cap.ROUND);pen.setStrokeJoin(Paint.Join.ROUND);pen.setStyle(Paint.Style.STROKE);}
    private void lines(Canvas c,float... xy){Path p=new Path();p.moveTo(xy[0],xy[1]);for(int i=2;i<xy.length;i+=2)p.lineTo(xy[i],xy[i+1]);c.drawPath(p,pen);}
    @Override public void draw(Canvas canvas){int save=canvas.save();Rect b=getBounds();canvas.translate(b.left,b.top);canvas.scale(b.width()/24f,b.height()/24f);
        switch(kind){
            case "search":canvas.drawCircle(10.5f,10.5f,6.5f,pen);lines(canvas,15.5f,15.5f,21,21);break;
            case "grid":for(int y=3;y<=17;y+=7)for(int x=3;x<=17;x+=7)canvas.drawRect(x,y,x+4,y+4,pen);break;
            case "list":for(int y=5;y<=19;y+=7){lines(canvas,3,y,5,y);lines(canvas,9,y,21,y);}break;
            case "sort":lines(canvas,3,6,21,6);lines(canvas,3,12,16,12);lines(canvas,3,18,10,18);break;
            case "filter":lines(canvas,3,4,21,4,14,12,14,20,10,18,10,12,3,4);break;
            case "settings":canvas.drawCircle(12,12,4,pen);canvas.drawCircle(12,12,8,pen);for(int i=0;i<8;i++){double a=i*Math.PI/4;lines(canvas,12+(float)Math.cos(a)*8,12+(float)Math.sin(a)*8,12+(float)Math.cos(a)*10,12+(float)Math.sin(a)*10);}break;
            case "more":pen.setStyle(Paint.Style.FILL);for(int y=5;y<=19;y+=7)canvas.drawCircle(12,y,1.6f,pen);pen.setStyle(Paint.Style.STROKE);break;
            case "back":lines(canvas,12,4,4,12,12,20);lines(canvas,4,12,21,12);break;
            case "chevron":lines(canvas,9,6,15,12,9,18);break;
            case "folder":lines(canvas,3,6,10,6,12,8,21,8,21,20,3,20,3,6);break;
            case "language":canvas.drawCircle(12,12,9,pen);canvas.drawOval(8,3,16,21,pen);lines(canvas,3,12,21,12);lines(canvas,5,7,19,7);lines(canvas,5,17,19,17);break;
            case "screen":canvas.drawRoundRect(6,2,18,22,1,1,pen);lines(canvas,10,19,14,19);break;
            case "image":canvas.drawRoundRect(3,3,21,21,1,1,pen);canvas.drawCircle(8,8,1.5f,pen);lines(canvas,4,18,10,12,13,15,16,11,20,16);break;
            case "controls":canvas.drawRoundRect(2,6,22,19,4,4,pen);lines(canvas,6,10,6,15);lines(canvas,3.5f,12.5f,8.5f,12.5f);canvas.drawCircle(16,11,0.8f,pen);canvas.drawCircle(19,14,0.8f,pen);break;
            case "audio":lines(canvas,3,9,7,9,12,5,12,19,7,15,3,15,3,9);canvas.drawArc(9,5,23,19,-55,110,false,pen);canvas.drawArc(12,8,20,16,-55,110,false,pen);break;
            case "volume":for(int i=0;i<4;i++)lines(canvas,4+i*5,20,4+i*5,17-i*4);break;
            case "performance":canvas.drawArc(3,4,21,22,155,230,false,pen);lines(canvas,12,14,17,8);canvas.drawCircle(12,14,1.5f,pen);break;
            case "refresh":canvas.drawArc(4,4,20,20,40,290,false,pen);lines(canvas,20,3,20,9,14,9);break;
            case "info":canvas.drawCircle(12,12,9,pen);lines(canvas,12,10,12,17);canvas.drawCircle(12,6.5f,0.5f,pen);break;
            case "music":lines(canvas,9,18,9,5,20,3,20,16);canvas.drawOval(3,16,9,21,pen);canvas.drawOval(14,14,20,19,pen);break;
            default:canvas.drawRoundRect(4,4,20,20,3,3,pen);break;
        }canvas.restoreToCount(save);
    }
    @Override public int getIntrinsicWidth(){return size;}
    @Override public int getIntrinsicHeight(){return size;}
    @Override public void setAlpha(int alpha){pen.setAlpha(alpha);invalidateSelf();}
    @Override public void setColorFilter(ColorFilter filter){pen.setColorFilter(filter);invalidateSelf();}
    @Override public int getOpacity(){return PixelFormat.TRANSLUCENT;}
}
