package io.github.brew.handset;

import java.util.Set;
import java.util.Locale;

/** Regras de apresentação sem acesso a arquivos nem dependência do Android. */
final class LibraryRules {
    static String formatSize(long bytes){
        if(bytes<0)return "";
        if(bytes<1024)return bytes+" B";
        if(bytes<1024L*1024)return Math.round(bytes/1024.0)+" KB";
        double size=bytes/(1024.0*1024.0);String unit="MB";
        for(String next:new String[]{"GB","TB","PB","EB"}){if(size<1024)break;size/=1024;unit=next;}
        return String.format(Locale.ROOT,"%.2f %s",size,unit);
    }
    static boolean matches(String resolution,String company,Set<String> hiddenResolutions,Set<String> hiddenCompanies){
        return !hiddenResolutions.contains(resolution==null?"Unknown":resolution)
            &&!hiddenCompanies.contains(company==null||company.trim().isEmpty()?"Unknown":company);
    }
}
