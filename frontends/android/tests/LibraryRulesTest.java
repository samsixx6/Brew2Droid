package io.github.brew.handset;

import java.util.*;

public final class LibraryRulesTest {
    static void check(boolean ok,String message){if(!ok)throw new AssertionError(message);}
    public static void main(String[] args){
        check(LibraryRules.formatSize(256410).equals("250 KB"),"KB must round to whole numbers");
        check(LibraryRules.formatSize(6L*1024*1024).equals("6.00 MB"),"MB must keep two decimals");
        check(LibraryRules.formatSize(6333399).equals("6.04 MB"),"MB precision was lost");
        check(LibraryRules.formatSize(-1).equals(""),"Unknown size must stay unknown");
        Set<String> resolutions=new HashSet<>(),companies=new HashSet<>();
        check(LibraryRules.matches("240×320","Gameloft",resolutions,companies),"All categories must start included");
        resolutions.add("240×320");
        check(!LibraryRules.matches("240×320","Gameloft",resolutions,companies),"Unchecked resolution must hide matching game");
        check(LibraryRules.matches("800×480","Gameloft",resolutions,companies),"A newly discovered resolution must remain included");
        companies.add("Gameloft");
        check(!LibraryRules.matches("800×480","Gameloft",resolutions,companies),"Company and resolution filters must combine");
        check(LibraryRules.matches("800×480",null,resolutions,companies),"Unknown companies remain visible by default");
        companies.add("Unknown");
        check(!LibraryRules.matches("800×480",null,resolutions,companies),"Unknown companies can be excluded explicitly");
        System.out.println("PASS: 10 library formatting/filter cases");
    }
}
