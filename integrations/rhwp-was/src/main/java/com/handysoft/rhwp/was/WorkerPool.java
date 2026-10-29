package com.handysoft.rhwp.was;

import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.logging.Level;
import java.util.logging.Logger;

import org.json.JSONObject;

/**
 * rhwp 워커 프로세스 풀.
 *
 * <ul>
 *   <li>워커 K 개를 미리 띄워 두고 요청마다 하나를 빌려 쓴다.</li>
 *   <li>빌릴 워커가 없으면 {@code queueTimeoutMs} 까지 기다렸다가 {@link PoolBusyException} — 호출자는 503 을 돌려준다.</li>
 *   <li>요청 중 워커가 죽거나 시간을 넘기면 그 워커만 버리고 새로 띄운다. 다른 워커·WAS 에는 영향이 없다.</li>
 * </ul>
 */
public final class WorkerPool {
    private static final Logger LOG = Logger.getLogger(WorkerPool.class.getName());

    private final RhwpConfig config;
    private final BlockingQueue<RhwpWorker> idle = new LinkedBlockingQueue<>();
    private final List<RhwpWorker> all = new ArrayList<>();
    private final AtomicInteger nextIndex = new AtomicInteger();
    private final AtomicInteger restarts = new AtomicInteger();
    private final AtomicInteger failures = new AtomicInteger();
    private volatile boolean closed;

    public WorkerPool(RhwpConfig config) throws IOException {
        this.config = config;
        for (int i = 0; i < config.workers; i++) {
            RhwpWorker w = spawn();
            idle.offer(w);
        }
    }

    private RhwpWorker spawn() throws IOException {
        RhwpWorker w = new RhwpWorker(nextIndex.getAndIncrement(), config.binary, config.workDir);
        if (!w.ping(config.timeoutMs)) {
            w.kill();
            throw new IOException("rhwp worker 기동 확인(ping) 실패: " + config.binary);
        }
        synchronized (all) {
            all.add(w);
        }
        return w;
    }

    private void discard(RhwpWorker w) {
        w.kill();
        synchronized (all) {
            all.remove(w);
        }
        restarts.incrementAndGet();
        try {
            idle.offer(spawn());
        } catch (IOException e) {
            // 다시 띄우지 못하면 풀이 하나 줄어든다. 다음 요청에서 또 시도한다.
            LOG.log(Level.SEVERE, "워커 재기동 실패", e);
            failures.incrementAndGet();
        }
    }

    /** 요청 한 건을 워커에게 보내고 응답을 받는다. */
    public JSONObject call(JSONObject request) throws PoolBusyException, RhwpWorker.WorkerException {
        if (closed) {
            throw new PoolBusyException("풀이 닫혔습니다");
        }
        RhwpWorker w = acquireLive();
        if (w == null) {
            // 워커가 전부 죽어 비어 있으면 하나 띄워 본다
            int live;
            synchronized (all) {
                live = all.size();
            }
            if (live == 0) {
                try {
                    w = spawn();
                } catch (IOException e) {
                    throw new PoolBusyException("워커가 없고 재기동도 실패: " + e.getMessage());
                }
            } else {
                throw new PoolBusyException("워커가 모두 사용 중입니다 (" + config.queueTimeoutMs + "ms 대기)");
            }
        }
        try {
            JSONObject response = w.call(request, config.timeoutMs);
            if (w.isAlive()) {
                idle.offer(w);
            } else {
                discard(w);
            }
            return response;
        } catch (RhwpWorker.WorkerException e) {
            failures.incrementAndGet();
            discard(w);
            throw e;
        }
    }

    /**
     * 살아 있는 빈 워커를 하나 빌린다. 밖에서 죽은(외부 kill·크래시) 워커가 큐에 남아 있으면
     * 그 자리에서 버리고 새로 띄운 뒤 다음 것을 본다 — 죽은 워커 때문에 요청 하나를 503 으로
     * 잃지 않기 위해서다. 대기 상한을 넘기면 null.
     */
    private RhwpWorker acquireLive() throws PoolBusyException {
        long deadline = System.nanoTime() + TimeUnit.MILLISECONDS.toNanos(config.queueTimeoutMs);
        while (true) {
            long remaining = Math.max(0L, TimeUnit.NANOSECONDS.toMillis(deadline - System.nanoTime()));
            RhwpWorker w;
            try {
                w = idle.poll(remaining, TimeUnit.MILLISECONDS);
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
                throw new PoolBusyException("대기 중단");
            }
            if (w == null) {
                return null;
            }
            if (w.isAlive()) {
                return w;
            }
            LOG.warning("worker-" + w.index() + " 가 밖에서 종료됨 — 교체");
            discard(w);
        }
    }

    public JSONObject stats() {
        int total;
        long requests = 0;
        synchronized (all) {
            total = all.size();
            for (RhwpWorker w : all) {
                requests += w.requestCount();
            }
        }
        return new JSONObject()
                .put("configured", config.workers)
                .put("alive", total)
                .put("idle", idle.size())
                .put("restarts", restarts.get())
                .put("failures", failures.get())
                .put("requests", requests);
    }

    public void close() {
        closed = true;
        List<RhwpWorker> snapshot;
        synchronized (all) {
            snapshot = new ArrayList<>(all);
            all.clear();
        }
        idle.clear();
        for (RhwpWorker w : snapshot) {
            w.shutdown(2000);
        }
    }

    public static final class PoolBusyException extends Exception {
        private static final long serialVersionUID = 1L;

        PoolBusyException(String message) {
            super(message);
        }
    }
}
