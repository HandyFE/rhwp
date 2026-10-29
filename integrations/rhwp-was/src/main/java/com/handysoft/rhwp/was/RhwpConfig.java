package com.handysoft.rhwp.was;

import java.io.File;

import javax.servlet.ServletContext;

/**
 * 컨텍스트 설정. 우선순위: 시스템 속성(-Drhwp.x) > 환경 변수(RHWP_X) > web.xml context-param.
 */
public final class RhwpConfig {
    public final File binary;
    public final int workers;
    public final long timeoutMs;
    public final long queueTimeoutMs;
    public final File workDir;
    public final String token;

    private RhwpConfig(File binary, int workers, long timeoutMs, long queueTimeoutMs, File workDir, String token) {
        this.binary = binary;
        this.workers = workers;
        this.timeoutMs = timeoutMs;
        this.queueTimeoutMs = queueTimeoutMs;
        this.workDir = workDir;
        this.token = token;
    }

    public static RhwpConfig load(ServletContext ctx) {
        String binary = get(ctx, "rhwp.binary");
        if (binary == null) {
            throw new IllegalStateException("rhwp.binary 설정이 없습니다 (-Drhwp.binary=<rhwp 실행 파일> 또는 RHWP_BINARY)");
        }
        File bin = new File(binary);
        if (!bin.isFile()) {
            throw new IllegalStateException("rhwp 실행 파일이 없습니다: " + bin.getAbsolutePath());
        }
        String workDirValue = get(ctx, "rhwp.workDir");
        File workDir = workDirValue != null ? new File(workDirValue) : (File) ctx.getAttribute(ServletContext.TEMPDIR);
        if (workDir == null) {
            workDir = new File(System.getProperty("java.io.tmpdir"));
        }
        workDir = new File(workDir, "rhwp-work");
        if (!workDir.isDirectory() && !workDir.mkdirs()) {
            throw new IllegalStateException("작업 폴더를 만들 수 없습니다: " + workDir.getAbsolutePath());
        }
        return new RhwpConfig(
                bin,
                Math.max(1, intOf(get(ctx, "rhwp.workers"), 2)),
                Math.max(1000L, longOf(get(ctx, "rhwp.timeoutMs"), 20000L)),
                Math.max(0L, longOf(get(ctx, "rhwp.queueTimeoutMs"), 5000L)),
                workDir,
                get(ctx, "rhwp.token"));
    }

    private static String get(ServletContext ctx, String key) {
        String v = System.getProperty(key);
        if (isBlank(v)) {
            v = System.getenv(key.toUpperCase().replace('.', '_'));
        }
        if (isBlank(v)) {
            v = ctx.getInitParameter(key);
        }
        return isBlank(v) ? null : v.trim();
    }

    private static boolean isBlank(String s) {
        return s == null || s.trim().isEmpty();
    }

    private static int intOf(String s, int def) {
        try {
            return s == null ? def : Integer.parseInt(s);
        } catch (NumberFormatException e) {
            return def;
        }
    }

    private static long longOf(String s, long def) {
        try {
            return s == null ? def : Long.parseLong(s);
        } catch (NumberFormatException e) {
            return def;
        }
    }
}
