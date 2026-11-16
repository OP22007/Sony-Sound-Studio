# Local decompile harness

`Dump.java` decompiles the classes of a local APK whose names match a regular expression into one
`.java` file each, using the same JADX library REA bundles. It exists so protocol research can be
searched with ordinary text tools instead of one REA call per method. It never runs the APK.

```sh
javac -cp "$REA_JADX_MCP_JAR" -d out scripts/jadx-dump/Dump.java
java -Xmx8g -Dthr=8 -cp "out;$REA_JADX_MCP_JAR" Dump base.apk dump-dir '^(hj0\.|nj0\.)'
```

Use `.` as the pattern for everything (about 28,000 classes, several minutes). Keep the APK and the
dump outside the repository; both are private research inputs and are git-ignored.
