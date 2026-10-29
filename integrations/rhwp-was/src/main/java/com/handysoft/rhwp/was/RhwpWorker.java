package com.handysoft.rhwp.was;

import java.io.BufferedReader;
import java.io.File;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.OutputStreamWriter;
import java.io.Writer;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.logging.Level;
import java.util.logging.Logger;

import org.json.JSONObject;

/**
 * `rhwp worker` 프로세스 하나. stdin 으로 한 줄 JSON 을 보내고 stdout 한 줄을 받는다.
 *
 * <p>응답은 별도 스레드가 읽어 큐에 넣으므로 호출자는 시간 제한을 걸 수 있다. 시간을 넘기면
 * 프로세스를 강제 종료하고 {@link #isAlive()} 가 false 가 된다 — 풀이 새 워커로 바꾼다.
 * 한 워커는 한 번에 한 요청만 처리한다(호출자가 보장).
 */
public final class RhwpWorker {
    private static final Logger LOG = Logger.getLogger(RhwpWorker.class.getName());
    private static final String POISON = "\u0000<eof>";

    private final int index;
    private final Process process;
    private final Writer stdin;
    private final BlockingQueue<String> lines = new LinkedBlockingQueue<>();
    private final AtomicBoolean broken = new AtomicBoolean(false);
    private long requestCount;

    public RhwpWorker(int index, File binary, File workDir) throws IOException {
        this.index = index;
        ProcessBuilder pb = new ProcessBuilder(binary.getAbsolutePath(), "worker");
        pb.directory(workDir);
        pb.redirectErrorStream(false);
        this.process = pb.start();
        this.stdin = new OutputStreamWriter(process.getOutputStream(), StandardCharsets.UTF_8);
        startReader();
        startStderrDrain();
    }

    private void startReader() {
        Thread t = new Thread(() -> {
            try (BufferedReader r = new BufferedReader(new InputStreamReader(process.getInputStream(), StandardCharsets.UTF_8))) {
                String line;
                while ((line = r.readLine()) != null) {
                    lines.offer(line);
                }
            } catch (IOException e) {
                LOG.log(Level.FINE, "worker-" + index + " stdout 닫힘", e);
            } finally {
                broken.set(true);
                lines.offer(POISON);
            }
        }, "rhwp-worker-" + index + "-stdout");
        t.setDaemon(true);
        t.start();
    }

    private void startStderrDrain() {
        Thread t = new Thread(() -> {
            try (BufferedReader r = new BufferedReader(new InputStreamReader(process.getErrorStream(), StandardCharsets.UTF_8))) {
                String line;
                while ((line = r.readLine()) != null) {
                    LOG.warning("worker-" + index + " stderr: " + line);
                }
            } catch (IOException ignored) {
                // 프로세스 종료
            }
        }, "rhwp-worker-" + index + "-stderr");
        t.setDaemon(true);
        t.start();
    }

    public int index() {
        return index;
    }

    public boolean isAlive() {
        return !broken.get() && process.isAlive();
    }

    public long requestCount() {
        return requestCount;
    }

    /**
     * 요청 한 줄을 보내고 응답 한 줄을 기다린다.
     *
     * @throws WorkerException 시간 초과·프로세스 종료·응답 파싱 실패. 이 예외 뒤 워커는 죽은 것으로 본다.
     */
    public synchronized JSONObject call(JSONObject request, long timeoutMs) throws WorkerException {
        if (!isAlive()) {
            throw new WorkerException("worker-" + index + " 가 살아 있지 않습니다");
        }
        requestCount++;
        try {
            stdin.write(request.toString());
            stdin.write('\n');
            stdin.flush();
        } catch (IOException e) {
            kill();
            throw new WorkerException("worker-" + index + " stdin 쓰기 실패: " + e.getMessage());
        }
        String line;
        try {
            line = lines.poll(timeoutMs, TimeUnit.MILLISECONDS);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            kill();
            throw new WorkerException("worker-" + index + " 대기 중단");
        }
        if (line == null) {
            kill();
            throw new WorkerException("worker-" + index + " 응답 시간 초과 (" + timeoutMs + "ms) — 강제 종료");
        }
        if (POISON.equals(line)) {
            throw new WorkerException("worker-" + index + " 프로세스가 종료됐습니다 (exit=" + exitCodeOrNull() + ")");
        }
        try {
            return new JSONObject(line);
        } catch (RuntimeException e) {
            kill();
            throw new WorkerException("worker-" + index + " 응답 파싱 실패: " + abbreviate(line));
        }
    }

    /** 기동 확인. */
    public boolean ping(long timeoutMs) {
        try {
            JSONObject r = call(new JSONObject().put("ping", true), timeoutMs);
            return r.optBoolean("pong", false);
        } catch (WorkerException e) {
            LOG.warning(e.getMessage());
            return false;
        }
    }

    public void kill() {
        broken.set(true);
        try {
            stdin.close();
        } catch (IOException ignored) {
            // 이미 닫힘
        }
        process.destroyForcibly();
    }

    /** 정상 종료: stdin 을 닫으면 워커가 EOF 로 끝난다. 끝나지 않으면 강제 종료. */
    public void shutdown(long waitMs) {
        broken.set(true);
        try {
            stdin.close();
        } catch (IOException ignored) {
            // 이미 닫힘
        }
        try {
            if (!process.waitFor(waitMs, TimeUnit.MILLISECONDS)) {
                process.destroyForcibly();
            }
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            process.destroyForcibly();
        }
    }

    private String exitCodeOrNull() {
        try {
            return process.isAlive() ? "alive" : String.valueOf(process.exitValue());
        } catch (IllegalThreadStateException e) {
            return "?";
        }
    }

    private static String abbreviate(String s) {
        return s.length() > 200 ? s.substring(0, 200) + "…" : s;
    }

    public static final class WorkerException extends Exception {
        private static final long serialVersionUID = 1L;

        WorkerException(String message) {
            super(message);
        }
    }
}
