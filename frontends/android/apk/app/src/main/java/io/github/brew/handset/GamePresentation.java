package io.github.brew.handset;

import org.json.JSONObject;
import java.text.*;
import java.util.Locale;

/** Formatação dos dados já calculados na tarefa de IO; bind de linha nunca consulta arquivos. */
final class GamePresentation {
    static String size(JSONObject game,String unknown){
        if(game.isNull("folder_size_bytes"))return unknown;
        long bytes=game.optLong("folder_size_bytes",-1);if(bytes<0)return unknown;
        return LibraryRules.formatSize(bytes);
    }
    static String resolution(JSONObject game,String unknown){JSONObject selected=game.optJSONObject("resolution"),geometry=selected==null?null:selected.optJSONObject("geometry");return geometry==null?unknown:geometry.optInt("width")+"×"+geometry.optInt("height");}
    static String subtitle(JSONObject game,String unknownResolution,String unknownSize){return resolution(game,unknownResolution)+", "+size(game,unknownSize);}
    static String version(JSONObject game){return game.isNull("version")?"":game.optString("version","");}
}
