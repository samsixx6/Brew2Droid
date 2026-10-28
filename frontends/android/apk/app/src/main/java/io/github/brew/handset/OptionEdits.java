package io.github.brew.handset;

/** Intenções aceitas na UI sobrevivem aos callbacks intermediários da fila de escrita. */
final class OptionEdits {
    final java.util.Map<String,String> intended=new java.util.HashMap<>();
    boolean record(String field,String accepted,String next){String previous=intended.containsKey(field)?intended.get(field):accepted;if(previous.equals(next))return false;intended.put(field,next);return true;}
    void clear(){intended.clear();}
}
