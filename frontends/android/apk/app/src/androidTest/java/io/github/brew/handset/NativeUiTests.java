package io.github.brew.handset;

import android.app.*;
import android.content.*;
import android.os.Bundle;
import android.view.*;
import android.widget.*;
import org.json.*;
import java.io.*;
import java.lang.reflect.Method;
import java.nio.file.Files;

/** Prova a entrada Android e a ponte real, sem mock do parser nem do sistema de arquivos. */
public final class NativeUiTests extends Instrumentation {
    private void onMain(Runnable action) {
        final Throwable[] error={null};super.runOnMainSync(()->{try{action.run();}catch(Throwable e){error[0]=e;}});
        if(error[0]!=null)throw new AssertionError(error[0]);
    }
    private int checks;private String testCase="all";
    private void capture(Activity activity,String name) throws Exception {
        final Throwable[] failure={null};onMain(()->{try{
            View view=activity.getWindow().getDecorView();android.graphics.Bitmap bitmap=android.graphics.Bitmap.createBitmap(view.getWidth(),view.getHeight(),android.graphics.Bitmap.Config.ARGB_8888);
            view.draw(new android.graphics.Canvas(bitmap));try(FileOutputStream out=new FileOutputStream(new File(getTargetContext().getCacheDir(),name))){bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG,100,out);}bitmap.recycle();
        }catch(Throwable e){failure[0]=e;}});if(failure[0]!=null)throw new Exception(failure[0]);
    }
    private void check(boolean ok, String reason) { if (!ok) throw new AssertionError(reason); checks++; }
    @Override public void onCreate(Bundle arguments) { super.onCreate(arguments);if(arguments!=null)testCase=arguments.getString("case","all");start(); }
    private void writesSurvive(Activity screen,File config,boolean leaving) throws Exception {
        Class<?> base=getTargetContext().getClassLoader().loadClass("io.github.brew.handset.NativeUi");java.lang.reflect.Field field=base.getDeclaredField("io");field.setAccessible(true);
        java.util.concurrent.ExecutorService executor=(java.util.concurrent.ExecutorService)field.get(screen);
        java.util.concurrent.CountDownLatch entered=new java.util.concurrent.CountDownLatch(1),release=new java.util.concurrent.CountDownLatch(1);
        executor.execute(()->{entered.countDown();try{release.await(4,java.util.concurrent.TimeUnit.SECONDS);}catch(InterruptedException e){Thread.currentThread().interrupt();}});entered.await();
        final boolean[] wanted=new boolean[2];
        onMain(()->{Switch overlay=screen.getWindow().getDecorView().findViewWithTag("brew.setting.audio.enabled");Switch fps=screen.getWindow().getDecorView().findViewWithTag("brew.setting.debug.fps_counter");wanted[0]=!overlay.isChecked();wanted[1]=!fps.isChecked();overlay.performClick();fps.performClick();if(leaving)screen.finish();});release.countDown();
        boolean saved=false;long until=android.os.SystemClock.uptimeMillis()+5000;
        while(android.os.SystemClock.uptimeMillis()<until){JSONObject now=new JSONObject(new String(Files.readAllBytes(config.toPath()),"UTF-8"));if(now.getJSONObject("audio").getBoolean("enabled")==wanted[0]&&now.getJSONObject("debug").getBoolean("fps_counter")==wanted[1]){saved=true;break;}android.os.SystemClock.sleep(50);}
        check(saved,leaving?"Leaving Settings discarded queued edits":"Rapid settings edits were not persisted");
        if(!leaving){android.os.SystemClock.sleep(250);waitForIdleSync();onMain(()->{View speed=screen.getWindow().getDecorView().findViewWithTag("brew.setting.debug.speed");Switch fps=screen.getWindow().getDecorView().findViewWithTag("brew.setting.debug.fps_counter");check(speed==null,"Removed diagnostic controls are still visible");check(fps.isChecked()==wanted[1],"FPS switch shows stale state");});}
    }
    private JSONObject call(String method, String... args) throws Exception {
        Class<?> c = getTargetContext().getClassLoader().loadClass("io.github.brew.handset.NativeBridge");
        Class<?>[] types = new Class<?>[args.length]; java.util.Arrays.fill(types, String.class);
        Method m = c.getDeclaredMethod(method, types); m.setAccessible(true);
        return new JSONObject((String)m.invoke(null, (Object[])args));
    }
    private boolean hasText(View view,String text) {
        if(view instanceof TextView && text.contentEquals(((TextView)view).getText()))return true;
        if(view instanceof ViewGroup){ViewGroup group=(ViewGroup)view;for(int i=0;i<group.getChildCount();i++)if(hasText(group.getChildAt(i),text))return true;}
        return false;
    }
    private void redesignedLibrary(Context context,File dir) throws Exception {
        File games=new File(dir,"Games"),game=new File(games,"Any folder name/Favourites/More games/Library Test (176x220) 1.2.3"),module=new File(game,"12345/module.mod");
        module.getParentFile().mkdirs();Files.write(module.toPath(),new byte[16]);Files.write(new File(game,"12345.mif").toPath(),new byte[32]);
        Files.write(new File(module.getParentFile(),"assets.dat").toPath(),new byte[1024]);Files.write(new File(game,"readme.txt").toPath(),new byte[28]);
        JSONObject configured=call("update",new File(context.getFilesDir(),"zeebx.json").toString(),games.toString(),new JSONObject().put("roms_dir",games.toString()).put("language","en").toString());check(configured.optBoolean("ok"),"Fixture settings rejected: "+configured);
        Activity screen=startActivitySync(new Intent().setClassName(context,"io.github.brew.handset.MainActivity").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
        try {
            long until=android.os.SystemClock.uptimeMillis()+8000;boolean ready=false;
            while(android.os.SystemClock.uptimeMillis()<until){final boolean[] found={false};onMain(()->{View list=screen.getWindow().getDecorView().findViewWithTag("brew.library");found[0]=list instanceof AbsListView&&((AbsListView)list).getCount()==1;});if(found[0]){ready=true;break;}android.os.SystemClock.sleep(50);}
            if(!ready)capture(screen,"native-redesign-failure.png");check(ready,"Library fixture did not load");waitForIdleSync();
            onMain(()->{
                View root=screen.getWindow().getDecorView();
                check(hasText(root,"B2D"),"Library toolbar must show the compact B2D name");
                check(hasText(root,"176×220, 1 KB"),"Library must show resolution and the whole 1100-byte package, including root MIF and nested assets");
                check(hasText(root,"Library Test"),"Library must use the readable outer game-folder title");
                check(hasText(root,"1.2.3"),"Library must show the game version separately");
                for(String tag:new String[]{"search","sort","filter","layout","settings"})check(root.findViewWithTag("brew.action."+tag)!=null,"Toolbar action missing: "+tag);
                ViewGroup toolbar=root.findViewWithTag("brew.library.toolbar");
                for(String tag:new String[]{"search","sort","filter","layout","settings"}){View action=root.findViewWithTag("brew.action."+tag);check(action.getParent()==toolbar&&action.getWidth()>0&&action.getRight()<=toolbar.getWidth(),"Toolbar icon must fit beside B2D: "+tag);}
                View search=root.findViewWithTag("brew.action.search");check(search!=null,"Search action is missing");search.performClick();
                EditText query=root.findViewWithTag("brew.library.search");query.setText("no-such-game");
                check(((AbsListView)root.findViewWithTag("brew.library")).getCount()==0,"Search did not filter the library");query.setText("Library Test");
                check(((AbsListView)root.findViewWithTag("brew.library")).getCount()==1,"Clearing search lost the game");
                View layout=root.findViewWithTag("brew.action.layout");check(layout!=null,"Layout action is missing");layout.performClick();
                check(root.findViewWithTag("brew.library") instanceof GridView,"Grid action did not switch the native library view");check(((GridView)root.findViewWithTag("brew.library")).getNumColumns()==3,"Grid must show three games per row");layout.performClick();
                check(root.findViewWithTag("brew.library") instanceof ListView,"List action did not restore the native library view");
            });
            capture(screen,"native-redesign-library.png");
        } finally {onMain(screen::finish);waitForIdleSync();}
    }
    @Override public void onStart() {
        Bundle result = new Bundle();int resultCode=Activity.RESULT_OK;Activity activity = null;File actualConfig=null;byte[] actualBefore=null;
        try {
            Context context = getTargetContext();
            actualConfig=new File(context.getFilesDir(),"zeebx.json");if(actualConfig.exists())actualBefore=Files.readAllBytes(actualConfig.toPath());
            Intent main = new Intent().setClassName(context, "io.github.brew.handset.MainActivity").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            check(context.getPackageManager().resolveActivity(main, 0) != null, "Native library activity is missing");
            File dir = new File(context.getCacheDir(), "native-ui-test-"+android.os.SystemClock.uptimeMillis()); dir.mkdirs();
            if(testCase.equals("redesign")){redesignedLibrary(context,dir);result.putString("stream","PASS: "+checks+" redesigned library checks\n");result.putInt("checks",checks);return;}
            File config = new File(dir, "preferências de teste.json");
            Files.write(config.toPath(), "{\"audio\":{\"volume\":80},\"future\":{\"keep\":42},\"graphics\":{\"resolucao_interna\":7}}".getBytes("UTF-8"));
            check(call("snapshot", config.toString(), dir.toString()).getBoolean("ok"), "JNI snapshot failed");
            check(call("update", config.toString(), dir.toString(), "{\"audio\":{\"volume\":31}}").getBoolean("ok"), "JNI update failed");
            JSONObject saved = new JSONObject(new String(Files.readAllBytes(config.toPath()), "UTF-8"));
            check(saved.getJSONObject("future").getInt("keep") == 42 && saved.getJSONObject("graphics").getInt("resolucao_interna") == 7, "Hidden fields lost");
            check(saved.getJSONObject("audio").getInt("volume") == 31, "Supported value not saved");
            byte[] before = Files.readAllBytes(config.toPath());
            check(!call("update", config.toString(), dir.toString(), "{\"audio\":{\"volume\":101}}").getBoolean("ok"), "Invalid volume accepted");
            check(java.util.Arrays.equals(before, Files.readAllBytes(config.toPath())), "Rejected edit changed settings");
            File games = new File(dir, "games"); games.mkdirs();
            File profiles=new File(dir,"display-profiles");
            check(call("scan", games.toString(), new File(dir,"art").toString(),profiles.toString()).getJSONObject("data").getJSONArray("games").length() == 0, "Empty folder not represented");
            File module=new File(games,"Mod/more games/variant_176x220/39367/game.mod");module.getParentFile().mkdirs();Files.write(module.toPath(),new byte[16]);
            JSONObject detected=call("scan",games.toString(),new File(dir,"art").toString(),profiles.toString()).getJSONObject("data").getJSONArray("games").getJSONObject(0).getJSONObject("resolution");
            check(detected.getJSONObject("geometry").getInt("width")==176&&detected.getString("source").equals("name"),"New game resolution was not detected");
            JSONObject manual=call("gameDisplay",profiles.toString(),module.toString(),"{\"width\":320,\"height\":240,\"depth\":16}").getJSONObject("data");
            check(manual.getString("source").equals("manual")&&manual.getJSONObject("geometry").getInt("width")==320,"Manual resolution JNI failed");
            check(call("gameDisplay",profiles.toString(),module.toString(),"").getJSONObject("data").getJSONObject("geometry").getInt("height")==240,"Manual resolution was not remembered");
            check(!call("gameDisplay",profiles.toString(),module.toString(),"{\"width\":0,\"height\":240,\"depth\":16}").getBoolean("ok"),"Invalid resolution accepted");
            check(call("gameDisplay",profiles.toString(),module.toString(),"null").getJSONObject("data").getString("source").equals("name"),"Auto resolution could not be restored");
            check(java.util.Arrays.equals(Files.readAllBytes(module.toPath()),new byte[16]),"Display discovery changed the game");
            if(!testCase.equals("bridge")) {
            activity = startActivitySync(main); waitForIdleSync();
            final Activity library = activity;
            onMain(() -> check(library.getWindow().getDecorView().findViewWithTag("brew.library") instanceof ListView, "Library is not a native list"));
            android.os.SystemClock.sleep(1200);waitForIdleSync();capture(library,"native-library.png");
            onMain(library::finish); activity = null; waitForIdleSync();
            Intent settings = new Intent().setClassName(context,"io.github.brew.handset.SettingsActivity").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            activity = startActivitySync(settings); waitForIdleSync();
            final Activity screen = activity;
            long until = android.os.SystemClock.uptimeMillis() + 6000;
            while (android.os.SystemClock.uptimeMillis() < until) {
                final boolean[] ready = {false}; onMain(() -> ready[0] = screen.getWindow().getDecorView().findViewWithTag("brew.setting.debug.fps_counter") != null);
                if (ready[0]) break; android.os.SystemClock.sleep(100);
            }
            onMain(() -> {
                View toggle = screen.getWindow().getDecorView().findViewWithTag("brew.setting.debug.fps_counter");
                check(toggle instanceof Switch, "Settings do not expose a native FPS switch");
                ViewGroup row = (ViewGroup)toggle.getParent(); View label = row.getChildAt(1);
                check(label.getWidth()>0&&toggle.getWidth()>0, "Settings layout was not measured");
                check(label.getRight() <= toggle.getLeft(), "Label overlaps switch");
                View decor=screen.getWindow().getDecorView();
                check(hasText(decor,"Profiles"),"Profile settings entry is missing");
                for(String old:new String[]{"controles_na_tela.modo","graphics.smooth","handset_keypad.transparency"})check(decor.findViewWithTag("brew.setting."+old)==null,"Profile-owned option remains global: "+old);
                Switch vibration=decor.findViewWithTag("brew.setting.handset_keypad.vibration");
                check(vibration!=null,"Global vibration switch is missing");vibration.setChecked(false);
            });
            long savedUntil=android.os.SystemClock.uptimeMillis()+5000;boolean savedVibration=false;
            while(android.os.SystemClock.uptimeMillis()<savedUntil){JSONObject now=new JSONObject(new String(Files.readAllBytes(actualConfig.toPath()),"UTF-8"));if(now.optJSONObject("handset_keypad")!=null&&!now.getJSONObject("handset_keypad").optBoolean("vibration",true)){savedVibration=true;break;}android.os.SystemClock.sleep(50);}
            check(savedVibration,"Global vibration switch did not persist");
            capture(screen,"native-settings.png");
            onMain(()->{
                Switch toggle=(Switch)screen.getWindow().getDecorView().findViewWithTag("brew.setting.debug.fps_counter");
                View scroll=screen.getWindow().getDecorView().findViewWithTag("brew.settings.scroll");((ScrollView)scroll).scrollTo(0,((View)toggle.getParent()).getTop());
            });
            waitForIdleSync();capture(screen,"native-settings-debug.png");
            if(!testCase.equals("queued"))writesSurvive(screen,actualConfig,false);
            if(!testCase.equals("callbacks"))writesSurvive(screen,actualConfig,true);
            }
            config.delete();
            result.putString("stream", "PASS: " + checks + " native "+(testCase.equals("bridge")?"JNI":"UI/JNI")+" checks\n"); result.putInt("checks", checks);
        } catch (Throwable error) {
            result.putString("stream", "FAIL: " + error + "\n"); result.putString("error",error.toString());resultCode=Activity.RESULT_CANCELED;
        } finally {
            if (activity != null) { final Activity a=activity; onMain(a::finish);waitForIdleSync(); }
            try{if(actualConfig!=null){if(actualBefore!=null){File temp=new File(actualConfig.getParentFile(),".native-ui-test-restore");Files.write(temp.toPath(),actualBefore);Files.move(temp.toPath(),actualConfig.toPath(),java.nio.file.StandardCopyOption.REPLACE_EXISTING,java.nio.file.StandardCopyOption.ATOMIC_MOVE);if(!java.util.Arrays.equals(actualBefore,Files.readAllBytes(actualConfig.toPath())))throw new IOException("Restored settings differ");}else Files.deleteIfExists(actualConfig.toPath());}}
            catch(Exception error){result.putString("stream","FAIL: cannot restore settings: "+error);resultCode=Activity.RESULT_CANCELED;}
            if(testCase.equals("redesign"))finish(resultCode,result);
        }
        // finish pode encerrar o processo: restaurar e conferir os dados antes de reportar.
        finish(resultCode,result);
    }
}
