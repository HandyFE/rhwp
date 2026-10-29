package com.handysoft.rhwp.was;

import java.io.IOException;

import javax.servlet.ServletException;
import javax.servlet.annotation.WebServlet;
import javax.servlet.http.HttpServlet;
import javax.servlet.http.HttpServletRequest;
import javax.servlet.http.HttpServletResponse;

import org.json.JSONObject;

/** GET /health — 풀 상태. 풀이 없으면 503 과 기동 실패 원인. */
@WebServlet(urlPatterns = "/health", name = "rhwpHealth")
public final class HealthServlet extends HttpServlet {
    private static final long serialVersionUID = 1L;

    @Override
    protected void doGet(HttpServletRequest req, HttpServletResponse resp) throws ServletException, IOException {
        WorkerPool pool = (WorkerPool) getServletContext().getAttribute(AppListener.POOL);
        RhwpConfig config = (RhwpConfig) getServletContext().getAttribute(AppListener.CONFIG);
        JSONObject body = new JSONObject().put("service", "rhwp").put("apiVersion", 1);
        if (pool == null) {
            body.put("ok", false).put("error", String.valueOf(getServletContext().getAttribute(AppListener.INIT_ERROR)));
            resp.setStatus(HttpServletResponse.SC_SERVICE_UNAVAILABLE);
        } else {
            JSONObject stats = pool.stats();
            body.put("ok", stats.getInt("alive") > 0)
                    .put("workers", stats)
                    .put("binary", config.binary.getAbsolutePath())
                    .put("timeoutMs", config.timeoutMs);
            resp.setStatus(stats.getInt("alive") > 0 ? HttpServletResponse.SC_OK : HttpServletResponse.SC_SERVICE_UNAVAILABLE);
        }
        resp.setContentType("application/json;charset=UTF-8");
        resp.getWriter().write(body.toString());
    }
}
