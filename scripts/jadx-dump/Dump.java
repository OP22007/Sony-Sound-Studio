import jadx.api.*;
import java.io.*;
import java.nio.file.*;
import java.util.*;
import java.util.regex.*;

public class Dump {
  public static void main(String[] a) throws Exception {
    JadxArgs args = new JadxArgs();
    args.setInputFile(new File(a[0]));
    args.setSkipResources(true);
    args.setThreadsCount(Integer.parseInt(System.getProperty("thr","1")));
    Pattern p = Pattern.compile(a[2]);
    Files.createDirectories(Paths.get(a[1]));
    try (JadxDecompiler jd = new JadxDecompiler(args)) {
      jd.load();
      int n = 0;
      for (JavaClass c : jd.getClasses()) {
        String name = c.getFullName();
        if (!p.matcher(name).find()) continue;
        String code;
        try { code = c.getCode(); } catch (Throwable t) { code = "// decompile failed: " + t; }
        Files.writeString(Paths.get(a[1], name + ".java"), code);
        n++;
      }
      System.out.println("dumped " + n);
    }
  }
}
