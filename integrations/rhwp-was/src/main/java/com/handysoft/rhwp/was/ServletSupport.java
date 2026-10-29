package com.handysoft.rhwp.was;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.StandardCopyOption;

import javax.servlet.ServletContext;
import javax.servlet.http.HttpServletRequest;
import javax.servlet.http.HttpServletResponse;
import javax.servlet.http.Part;

import org.json.JSONObject;

/** 서블릿 공통: 풀·설정 조회, 토큰 검사, multipart 파트 읽기, JSON 응답. */
final class ServletSupport {
    private ServletSupport() {
    }

    /** 풀·토큰 검사. 문제가 있으면 응답을 쓰고 null 을 돌려준다. */
    static WorkerPool poolOrReject(ServletContext ctx, HttpServletRequest req, HttpServletResponse resp) throws IOException {
        WorkerPool pool = (WorkerPool) ctx.getAttribute(AppListener.POOL);
        RhwpConfig config = (RhwpConfig) ctx.getAttribute(AppListener.CONFIG);
        if (pool == null) {
            json(resp, HttpServletResponse.SC_SERVICE_UNAVAILABLE, error("워커 풀이 기동되지 않았습니다: " + ctx.getAttribute(AppListener.INIT_ERROR)));
            return null;
        }
        if (config.token != null && !config.token.equals(req.getHeader("X-Rhwp-Token"))) {
            json(resp, HttpServletResponse.SC_UNAUTHORIZED, error("X-Rhwp-Token 불일치"));
            return null;
        }
        return pool;
    }

    static String partText(Part part) throws IOException {
        if (part == null) {
            return null;
        }
        try (InputStream in = part.getInputStream()) {
            byte[] buf = new byte[8192];
            ByteArrayOutputStream bos = new ByteArrayOutputStream();
            int n;
            while ((n = in.read(buf)) > 0) {
                bos.write(buf, 0, n);
            }
            return new String(bos.toByteArray(), StandardCharsets.UTF_8);
        }
    }

    static void savePart(Part part, File target) throws IOException {
        try (InputStream in = part.getInputStream()) {
            Files.copy(in, target.toPath(), StandardCopyOption.REPLACE_EXISTING);
        }
    }

    static String extensionOf(String name) {
        if (name != null && name.toLowerCase().endsWith(".hwpx")) {
            return "hwpx";
        }
        return "hwp";
    }

    static JSONObject error(String message) {
        return new JSONObject().put("ok", false).put("error", message);
    }

    static void json(HttpServletResponse resp, int status, JSONObject body) throws IOException {
        resp.setStatus(status);
        resp.setContentType("application/json;charset=UTF-8");
        resp.getWriter().write(body.toString());
    }

    /** 풀 호출의 예외를 503 으로 바꾼다. 성공하면 결과, 실패하면 응답을 쓰고 null. */
    static JSONObject callOrReject(WorkerPool pool, JSONObject request, HttpServletResponse resp, java.util.logging.Logger log) throws IOException {
        try {
            return pool.call(request);
        } catch (WorkerPool.PoolBusyException e) {
            json(resp, HttpServletResponse.SC_SERVICE_UNAVAILABLE, error(e.getMessage()));
            return null;
        } catch (RhwpWorker.WorkerException e) {
            log.log(java.util.logging.Level.WARNING, "워커 장애 id=" + request.optString("id"), e);
            json(resp, HttpServletResponse.SC_SERVICE_UNAVAILABLE, error(e.getMessage()));
            return null;
        }
    }
}
