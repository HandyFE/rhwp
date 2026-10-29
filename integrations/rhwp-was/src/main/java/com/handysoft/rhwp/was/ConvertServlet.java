package com.handysoft.rhwp.was;

import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.ArrayList;
import java.util.Base64;
import java.util.List;
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
 * POST /convert — HWP 파일에 액션을 적용해 수정된 파일을 돌려준다.
 *
 * <p>multipart/form-data:
 * <ul>
 *   <li>{@code file} — 원본 HWP/HWPX</li>
 *   <li>{@code actions} — JSON 배열. 각 항목은 BMS {@code CellAction} 과 같은 키
 *       ({@code action, fieldName, inputValue, fontName, fontSize, fontColor, fontBold, appendLine}).
 *       그림 액션({@code insertImage}·{@code appendPicture})은 {@code imagePart} 로 multipart 파트 이름을
 *       가리킨다. 파트가 없고 {@code inputValue} 가 http(s) URL 이면 설정 {@code rhwp.allowImageUrl=true}
 *       일 때만 내려받는다.</li>
 *   <li>{@code verify} — "false" 면 저장 후 재확인 생략(기본 true)</li>
 *   <li>{@code strictVerify} — "false" 면 재확인 불일치를 오류로 보지 않는다(기본 true)</li>
 * </ul>
 * 성공(200): 본문은 수정된 파일 바이트, 헤더 {@code X-Rhwp-Result} 에 워커 결과 JSON(UTF-8 → Base64).
 * 실패: 400(요청 오류)·422(문서·액션 처리·재확인 실패, JSON 본문)·503(워커 없음)·401(토큰 불일치).
 */
@WebServlet(urlPatterns = "/convert", name = "rhwpConvert")
@MultipartConfig(maxFileSize = 100L * 1024 * 1024, maxRequestSize = 110L * 1024 * 1024, fileSizeThreshold = 1024 * 1024)
public final class ConvertServlet extends HttpServlet {
    private static final long serialVersionUID = 1L;
    private static final Logger LOG = Logger.getLogger(ConvertServlet.class.getName());

    @Override
    protected void doPost(HttpServletRequest req, HttpServletResponse resp) throws ServletException, IOException {
        WorkerPool pool = ServletSupport.poolOrReject(getServletContext(), req, resp);
        if (pool == null) {
            return;
        }
        RhwpConfig config = (RhwpConfig) getServletContext().getAttribute(AppListener.CONFIG);

        Part filePart = req.getPart("file");
        String actionsRaw = ServletSupport.partText(req.getPart("actions"));
        if (filePart == null || filePart.getSize() == 0 || actionsRaw == null) {
            ServletSupport.json(resp, HttpServletResponse.SC_BAD_REQUEST, ServletSupport.error("multipart 'file' 과 'actions' 가 필요합니다"));
            return;
        }
        JSONArray actions;
        try {
            actions = new JSONArray(actionsRaw);
        } catch (JSONException e) {
            ServletSupport.json(resp, HttpServletResponse.SC_BAD_REQUEST, ServletSupport.error("actions 는 JSON 배열이어야 합니다: " + e.getMessage()));
            return;
        }
        boolean verify = !"false".equalsIgnoreCase(ServletSupport.partText(req.getPart("verify")));
        boolean strict = !"false".equalsIgnoreCase(ServletSupport.partText(req.getPart("strictVerify")));

        String id = UUID.randomUUID().toString();
        String ext = ServletSupport.extensionOf(filePart.getSubmittedFileName());
        File input = new File(config.workDir, id + "_in." + ext);
        File output = new File(config.workDir, id + "_out." + ext);
        List<File> temps = new ArrayList<>();
        temps.add(input);
        temps.add(output);
        try {
            ServletSupport.savePart(filePart, input);
            String imageError = materializeImages(req, config, actions, id, temps);
            if (imageError != null) {
                ServletSupport.json(resp, HttpServletResponse.SC_BAD_REQUEST, ServletSupport.error(imageError));
                return;
            }
            JSONObject request = new JSONObject()
                    .put("id", id)
                    .put("op", "convert")
                    .put("input", input.getAbsolutePath())
                    .put("output", output.getAbsolutePath())
                    .put("actions", actions)
                    .put("verify", verify)
                    .put("strictVerify", strict);

            JSONObject result = ServletSupport.callOrReject(pool, request, resp, LOG);
            if (result == null) {
                return;
            }
            if (!result.optBoolean("ok", false) || !output.isFile()) {
                ServletSupport.json(resp, 422, result.put("ok", false));
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
                for (File f : temps) {
                    Files.deleteIfExists(f.toPath());
                }
            }
        }
    }

    /**
     * 그림 액션의 그림을 임시 파일로 만들고 액션에 {@code imagePath} 를 넣는다.
     * 문제가 있으면 오류 메시지를 돌려준다(없으면 null).
     */
    private static String materializeImages(HttpServletRequest req, RhwpConfig config, JSONArray actions, String id, List<File> temps)
            throws IOException, ServletException {
        for (int i = 0; i < actions.length(); i++) {
            JSONObject a = actions.optJSONObject(i);
            if (a == null) {
                return "actions[" + i + "] 는 객체여야 합니다";
            }
            String action = a.optString("action", "");
            if (!"insertImage".equals(action) && !"appendPicture".equals(action)) {
                continue;
            }
            if (a.has("imagePath")) {
                // 호출자가 서버 로컬 경로를 직접 주는 것은 받지 않는다(워커가 임의 파일을 읽게 된다).
                a.remove("imagePath");
            }
            File target = new File(config.workDir, id + "_img" + i + ".bin");
            temps.add(target);
            String partName = a.optString("imagePart", "");
            if (!partName.isEmpty()) {
                Part p = req.getPart(partName);
                if (p == null || p.getSize() == 0) {
                    return "actions[" + i + "]: multipart 파트 '" + partName + "' 가 없습니다";
                }
                if (p.getSize() > config.maxImageBytes) {
                    return "actions[" + i + "]: 그림이 상한(" + config.maxImageBytes + " bytes)을 넘습니다";
                }
                ServletSupport.savePart(p, target);
            } else {
                String url = a.optString("inputValue", "");
                if (!(url.startsWith("http://") || url.startsWith("https://"))) {
                    return "actions[" + i + "]: 그림 액션에는 imagePart(multipart 파트 이름) 또는 http(s) URL inputValue 가 필요합니다";
                }
                if (!config.allowImageUrl) {
                    return "actions[" + i + "]: URL 그림은 rhwp.allowImageUrl=true 일 때만 받습니다 (BMS 는 multipart 로 첨부)";
                }
                String err = download(url, target, config);
                if (err != null) {
                    return "actions[" + i + "]: " + err;
                }
            }
            a.put("imagePath", target.getAbsolutePath());
        }
        return null;
    }

    private static String download(String url, File target, RhwpConfig config) {
        try {
            HttpURLConnection conn = (HttpURLConnection) new URL(url).openConnection();
            conn.setConnectTimeout((int) config.imageUrlTimeoutMs);
            conn.setReadTimeout((int) config.imageUrlTimeoutMs);
            conn.setInstanceFollowRedirects(true);
            if (conn.getResponseCode() != 200) {
                return "그림 URL 응답 " + conn.getResponseCode() + ": " + url;
            }
            long total = 0;
            try (InputStream in = conn.getInputStream(); OutputStream out = Files.newOutputStream(target.toPath())) {
                byte[] buf = new byte[16384];
                int n;
                while ((n = in.read(buf)) > 0) {
                    total += n;
                    if (total > config.maxImageBytes) {
                        return "그림이 상한(" + config.maxImageBytes + " bytes)을 넘습니다: " + url;
                    }
                    out.write(buf, 0, n);
                }
            }
            return total == 0 ? "그림 URL 이 비어 있습니다: " + url : null;
        } catch (IOException e) {
            return "그림 URL 내려받기 실패: " + url + " - " + e.getMessage();
        }
    }
}
