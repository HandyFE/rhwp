package com.handysoft.rhwp.was;

import java.io.IOException;
import java.util.logging.Logger;

import javax.servlet.ServletContext;
import javax.servlet.ServletContextEvent;
import javax.servlet.ServletContextListener;

/** 컨텍스트 기동 시 워커 풀을 띄우고, 내릴 때 워커를 전부 정리한다(고아 프로세스 방지). */
public final class AppListener implements ServletContextListener {
    private static final Logger LOG = Logger.getLogger(AppListener.class.getName());
    static final String POOL = "rhwp.pool";
    static final String CONFIG = "rhwp.config";
    static final String INIT_ERROR = "rhwp.initError";

    @Override
    public void contextInitialized(ServletContextEvent sce) {
        ServletContext ctx = sce.getServletContext();
        try {
            RhwpConfig config = RhwpConfig.load(ctx);
            WorkerPool pool = new WorkerPool(config);
            ctx.setAttribute(CONFIG, config);
            ctx.setAttribute(POOL, pool);
            LOG.info("rhwp 워커 풀 기동: " + config.workers + "개, binary=" + config.binary + ", workDir=" + config.workDir);
        } catch (IOException | RuntimeException e) {
            // 컨텍스트 자체는 올라오게 두고(/health 가 원인을 알려 준다) 변환 요청은 503 으로 거절한다.
            ctx.setAttribute(INIT_ERROR, e.getMessage());
            LOG.severe("rhwp 워커 풀 기동 실패: " + e.getMessage());
        }
    }

    @Override
    public void contextDestroyed(ServletContextEvent sce) {
        ServletContext ctx = sce.getServletContext();
        WorkerPool pool = (WorkerPool) ctx.getAttribute(POOL);
        if (pool != null) {
            pool.close();
            ctx.removeAttribute(POOL);
            LOG.info("rhwp 워커 풀 종료");
        }
    }
}
