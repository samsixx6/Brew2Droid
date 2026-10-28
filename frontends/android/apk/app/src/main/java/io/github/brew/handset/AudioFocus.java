package io.github.brew.handset;

import android.app.Activity;
import android.media.AudioAttributes;
import android.media.AudioFocusRequest;
import android.media.AudioManager;
import android.os.Handler;
import android.os.Looper;

// A NativeActivity continua sendo a atividade. Só a política do AudioManager precisa de Java.
public final class AudioFocus {
    static { System.loadLibrary("zeebx_android"); }
    private static AudioFocusRequest request;
    private static AudioManager manager;
    private static native void changed(boolean granted);

    public static void setWanted(Activity activity, boolean wanted) {
        activity.runOnUiThread(() -> {
            if (manager == null) manager = (AudioManager) activity.getSystemService(Activity.AUDIO_SERVICE);
            if (manager == null) { changed(false); return; }
            if (request == null) {
                request = new AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
                    .setAudioAttributes(new AudioAttributes.Builder()
                        .setUsage(AudioAttributes.USAGE_GAME)
                        .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build())
                    .setAcceptsDelayedFocusGain(true)
                    .setWillPauseWhenDucked(true)
                    .setOnAudioFocusChangeListener(focus -> changed(focus == AudioManager.AUDIOFOCUS_GAIN),
                        new Handler(Looper.getMainLooper())).build();
            }
            if (wanted) changed(manager.requestAudioFocus(request) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED);
            else { manager.abandonAudioFocusRequest(request); changed(true); }
        });
    }
}
