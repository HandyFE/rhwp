package com.handysoft.rhwp.was;

import java.io.File;
import java.io.IOException;
import java.nio.file.Files;
import java.util.UUID;
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
 * 조회 API (문서를 바꾸지 않는다).
 *
 * <ul>
 *   <li>{@code POST /fields} — multipart {@code file}, {@code option}(0 셀 필드·1 누름틀·2 전부, 기본 0),
 *       {@code number}(0 없음·1 순번 {@code {{n}}}·2 개수). BMS {@code getFieldList} 용.
 *       응답 {@code {"ok":true,"names":[…],"fields":[…],"count":n}}.</li>
 *   <li>{@code POST /field-values} — multipart {@code file}, {@code fields}(JSON 배열 또는 쉼표 구분 이름).
 *       BMS {@code getFieldValues} 용. 응답 {@code {"ok":true,"values":{"miskey":"…"}}}(없으면 null).</li>
 * </ul>
 */
@WebServlet(urlPatterns = { "/fields", "/field-values" }, name = "rhwpFields")
@MultipartConfig(maxFileSize = 100L * 1024 * 1024, maxRequestSize = 110L * 1024 * 1024, fileSizeThreshold = 1024 * 1024)
public final class FieldsServlet extends HttpServlet {
    private static final long serialVersionUID = 1L;
    private static final Logger LOG = Logger.getLogger(FieldsServlet.class.getName());

    @Override
    protected void doPost(HttpServletRequest req, HttpServletResponse resp) throws ServletException, IOException {
        WorkerPool pool = ServletSupport.poolOrReject(getServletContext(), req, resp);
        if (pool == null) {
            return;
        }
        RhwpConfig config = (RhwpConfig) getServletContext().getAttribute(AppListener.CONFIG);
        boolean values = req.getServletPath().endsWith("/field-values");

        Part filePart = req.getPart("file");
        if (filePart == null || filePart.getSize() == 0) {
            ServletSupport.json(resp, HttpServletResponse.SC_BAD_REQUEST, ServletSupport.error("multipart 'file' 이 필요합니다"));
            return;
        }
        String id = UUID.randomUUID().toString();
        File input = new File(config.workDir, id + "_in." + ServletSupport.extensionOf(filePart.getSubmittedFileName()));
        try {
            ServletSupport.savePart(filePart, input);
            JSONObject request = new JSONObject().put("id", id).put("input", input.getAbsolutePath());
            if (values) {
                String raw = ServletSupport.partText(req.getPart("fields"));
                if (raw == null || raw.trim().isEmpty()) {
                    ServletSupport.json(resp, HttpServletResponse.SC_BAD_REQUEST, ServletSupport.error("'fields' 가 필요합니다 (JSON 배열 또는 쉼표 구분)"));
                    return;
                }
                JSONArray names;
                try {
                    names = raw.trim().startsWith("[") ? new JSONArray(raw) : new JSONArray(raw.split(","));
                } catch (JSONException e) {
                    ServletSupport.json(resp, HttpServletResponse.SC_BAD_REQUEST, ServletSupport.error("fields 파싱 실패: " + e.getMessage()));
                    return;
                }
                request.put("op", "values").put("fields", names);
            } else {
                request.put("op", "fields")
                        .put("option", intPart(req, "option", 0))
                        .put("number", intPart(req, "number", 0));
            }
            JSONObject result = ServletSupport.callOrReject(pool, request, resp, LOG);
            if (result == null) {
                return;
            }
            ServletSupport.json(resp, result.optBoolean("ok", false) ? HttpServletResponse.SC_OK : 422, result);
        } finally {
            if (!Boolean.getBoolean("rhwp.keepWork")) {
                Files.deleteIfExists(input.toPath());
            }
        }
    }

    private static int intPart(HttpServletRequest req, String name, int def) throws IOException, ServletException {
        String v = ServletSupport.partText(req.getPart(name));
        if (v == null || v.trim().isEmpty()) {
            return def;
        }
        try {
            return Integer.parseInt(v.trim());
        } catch (NumberFormatException e) {
            return def;
        }
    }
}
