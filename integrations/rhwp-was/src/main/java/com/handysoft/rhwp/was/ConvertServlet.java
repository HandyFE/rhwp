package com.handysoft.rhwp.was;

import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.StandardCopyOption;
import java.util.Base64;
import java.util.UUID;
import java.util.logging.Level;
import java.util.logging.Logger;

import javax.servlet.ServletException;
import javax.servlet.annotation.MultipartConfig;
import javax.servlet.annotation.WebServlet;
import javax.servlet.http.HttpServlet;
import javax.servlet.http.HttpServletRequest;
import javax.servlet.http.HttpServletResponse;
import javax.servlet.http.Part;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

/**
 * POST /convert — HWP 파일에 액션을 적용해 수정된 파일을 돌려준다.
 *
 * <p>multipart/form-data:
 * <ul>
 *   <li>{@code file} — 원본 HWP/HWPX</li>
 *   <li>{@code actions} — JSON 배열. 각 항목은 BMS {@code CellAction} 과 같은 키
 *       ({@code action, fieldName, inputValue, fontName, fontSize, fontColor, fontBold, appendLine})</li>
 *   <li>{@code verify} — "false" 면 저장 후 재확인 생략(기본 true)</li>
 * </ul>
 * 성공(200): 본문은 수정된 파일 바이트, 헤더 {@code X-Rhwp-Result} 에 워커 결과 JSON(UTF-8 → Base64).
 * 실패: 400(요청 오류)·422(문서·액션 처리 실패, JSON 본문)·503(워커 없음)·401(토큰 불일치).
 */
@WebServlet(urlPatterns = "/convert", name = "rhwpConvert")
@MultipartConfig(maxFileSize = 100L * 1024 * 1024, maxRequestSize = 110L * 1024 * 1024, fileSizeThreshold = 1024 * 1024)
public final class ConvertServlet extends HttpServlet {
    private static final long serialVersionUID = 1L;
    private static final Logger LOG = Logger.getLogger(ConvertServlet.class.getName());

    @Override
    protected void doPost(HttpServletRequest req, HttpServletResponse resp) throws ServletException, IOException {
        WorkerPool pool = (WorkerPool) getServletContext().getAttribute(AppListener.POOL);
        RhwpConfig config = (RhwpConfig) getServletContext().getAttribute(AppListener.CONFIG);
        if (pool == null) {
            json(resp, HttpServletResponse.SC_SERVICE_UNAVAILABLE, error("워커 풀이 기동되지 않았습니다: " + getServletContext().getAttribute(AppListener.INIT_ERROR)));
            return;
        }
        if (config.token != null && !config.token.equals(req.getHeader("X-Rhwp-Token"))) {
            json(resp, HttpServletResponse.SC_UNAUTHORIZED, error("X-Rhwp-Token 불일치"));
            return;
        }

        Part filePart = req.getPart("file");
        String actionsRaw = partText(req.getPart("actions"));
        if (filePart == null || filePart.getSize() == 0 || actionsRaw == null) {
            json(resp, HttpServletResponse.SC_BAD_REQUEST, error("multipart 'file' 과 'actions' 가 필요합니다"));
            return;
        }
        JSONArray actions;
        try {
            actions = new JSONArray(actionsRaw);
        } catch (JSONException e) {
            json(resp, HttpServletResponse.SC_BAD_REQUEST, error("actions 는 JSON 배열이어야 합니다: " + e.getMessage()));
            return;
        }
        boolean verify = !"false".equalsIgnoreCase(partText(req.getPart("verify")));

        String id = UUID.randomUUID().toString();
        String ext = extensionOf(filePart.getSubmittedFileName());
        File input = new File(config.workDir, id + "_in." + ext);
        File output = new File(config.workDir, id + "_out." + ext);
        try {
            try (InputStream in = filePart.getInputStream()) {
                Files.copy(in, input.toPath(), StandardCopyOption.REPLACE_EXISTING);
            }
            JSONObject request = new JSONObject()
                    .put("id", id)
                    .put("input", input.getAbsolutePath())
                    .put("output", output.getAbsolutePath())
                    .put("actions", actions)
                    .put("verify", verify);

            JSONObject result;
            try {
                result = pool.call(request);
            } catch (WorkerPool.PoolBusyException e) {
                json(resp, HttpServletResponse.SC_SERVICE_UNAVAILABLE, error(e.getMessage()));
                return;
            } catch (RhwpWorker.WorkerException e) {
                LOG.log(Level.WARNING, "워커 장애 id=" + id, e);
                json(resp, HttpServletResponse.SC_SERVICE_UNAVAILABLE, error(e.getMessage()));
                return;
            }

            if (!result.optBoolean("ok", false) || !output.isFile()) {
                json(resp, 422, result.put("ok", false));
                return;
            }
            resp.setStatus(HttpServletResponse.SC_OK);
            resp.setContentType("application/octet-stream");
            resp.setHeader("Content-Disposition", "attachment; filename=\"" + id + "." + ext + "\"");
            resp.setHeader("X-Rhwp-Result", Base64.getEncoder().encodeToString(result.toString().getBytes(StandardCharsets.UTF_8)));
            resp.setContentLengthLong(output.length());
            try (OutputStream out = resp.getOutputStream()) {
                Files.copy(output.toPath(), out);
            }
        } finally {
            // 작업 파일은 남기지 않는다. 디버깅이 필요하면 -Drhwp.keepWork=true
            if (!Boolean.getBoolean("rhwp.keepWork")) {
                Files.deleteIfExists(input.toPath());
                Files.deleteIfExists(output.toPath());
            }
        }
    }

    private static String partText(Part part) throws IOException {
        if (part == null) {
            return null;
        }
        try (InputStream in = part.getInputStream()) {
            byte[] buf = new byte[8192];
            java.io.ByteArrayOutputStream bos = new java.io.ByteArrayOutputStream();
            int n;
            while ((n = in.read(buf)) > 0) {
                bos.write(buf, 0, n);
            }
            return new String(bos.toByteArray(), StandardCharsets.UTF_8);
        }
    }

    private static String extensionOf(String name) {
        if (name != null && name.toLowerCase().endsWith(".hwpx")) {
            return "hwpx";
        }
        return "hwp";
    }

    private static JSONObject error(String message) {
        return new JSONObject().put("ok", false).put("error", message);
    }

    private static void json(HttpServletResponse resp, int status, JSONObject body) throws IOException {
        resp.setStatus(status);
        resp.setContentType("application/json;charset=UTF-8");
        resp.getWriter().write(body.toString());
    }
}
