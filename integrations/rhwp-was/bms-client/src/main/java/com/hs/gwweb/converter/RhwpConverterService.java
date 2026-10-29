package com.hs.gwweb.converter;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.ArrayList;
import java.util.Base64;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicLong;

import javax.annotation.PostConstruct;

import org.apache.commons.lang3.StringUtils;
import org.json.JSONArray;
import org.json.JSONObject;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.beans.factory.annotation.Qualifier;
import org.springframework.core.io.FileSystemResource;
import org.springframework.http.HttpEntity;
import org.springframework.http.HttpHeaders;
import org.springframework.http.HttpMethod;
import org.springframework.http.HttpStatus;
import org.springframework.http.MediaType;
import org.springframework.http.ResponseEntity;
import org.springframework.stereotype.Service;
import org.springframework.util.LinkedMultiValueMap;
import org.springframework.util.MultiValueMap;
import org.springframework.web.client.HttpStatusCodeException;
import org.springframework.web.client.ResourceAccessException;
import org.springframework.web.client.RestTemplate;

import com.hs.gwweb.common.GlobalsConfig;
import com.hs.gwweb.converter.action.AddPageCellAction;
import com.hs.gwweb.converter.action.AppendPictureAction;
import com.hs.gwweb.converter.action.AppendSignDateTextExAction;
import com.hs.gwweb.converter.action.AppendSignTextExAction;
import com.hs.gwweb.converter.action.AppendTextExAction;
import com.hs.gwweb.converter.action.CellAction;
import com.hs.gwweb.converter.action.DefaultAction;
import com.hs.gwweb.converter.action.DefaultSignAction;
import com.hs.gwweb.converter.action.GetTextValueAction;
import com.hs.gwweb.converter.action.InsertImageAction;
import com.hs.gwweb.converter.action.InsertSignDateTextExAction;
import com.hs.gwweb.converter.action.InsertSignTextExAction;
import com.hs.gwweb.converter.action.InsertTextAction;
import com.hs.gwweb.converter.action.InsertTextExAction;
import com.hs.gwweb.stub.util.DocFileUtils;

/**
 * rhwp converter — {@code rhwp.war}(별도 컨텍스트, 내부 rhwp 워커 풀)를 HTTP 로 호출하는 {@link DocConverter}.
 *
 * <p>설정(globals.properties):
 * <pre>
 * docconverter.use=true
 * docconverter.class=RhwpConverterService
 * docconverter.rhwp.url=http://127.0.0.1:8080/rhwp        # rhwp.war 컨텍스트 URL (끝에 / 없이)
 * docconverter.rhwp.token=                                 # rhwp.token 을 설정했으면 같은 값
 * docconverter.rhwp.fallback.class=HwpLibConverterService  # rhwp 실패 시 대체 구현체. 비우면 대체 없음
 * docconverter.rhwp.breaker.failures=3                     # 연속 실패 N 회면 우회
 * docconverter.rhwp.breaker.openms=300000                  # 우회 유지 시간(ms). 지나면 1건 시험
 * docconverter.rhwp.image.timeoutms=5000                   # 서명 이미지 URL 내려받기 시간 상한
 * docconverter.rhwp.verify=true                            # 저장 후 재확인(불일치면 실패 → 대체 경로)
 * </pre>
 * HTTP 시간 상한은 기존 {@code docconverter.http.readtimeoutms}·{@code connectiontimeoutms}
 * ({@code docConverterRestTemplate})를 그대로 쓴다.
 *
 * <p>장애 처리: 연결 실패·503·시간 초과·422(문서 처리 실패·재확인 불일치)는 전부 "rhwp 실패"로 보고
 * 대체 구현체를 호출한다. 대체 구현체도 실패하면 예외를 그대로 던진다(기존 호출처의 처리와 같다).
 * 서킷 브레이커가 열려 있는 동안은 rhwp 를 부르지 않고 바로 대체 구현체로 간다.
 */
@Service("RhwpConverterService")
public class RhwpConverterService implements DocConverter {
	static org.apache.logging.log4j.Logger logger = org.apache.logging.log4j.LogManager.getLogger(RhwpConverterService.class);

	@Autowired
	@Qualifier("docConverterRestTemplate")
	private RestTemplate restTemplate;

	@Autowired
	@Qualifier("HwpLibConverterService")
	private DocConverter hwpLibDocConverter;
	@Autowired
	@Qualifier("HwpDocConverterService")
	private DocConverter hwpDocDocConverter;
	@Autowired
	@Qualifier("HsCellDocConverterService")
	private DocConverter hsCellDocConverter;

	private String baseUrl;
	private String token;
	private DocConverter fallback;
	private String fallbackName;
	private int breakerFailures;
	private long breakerOpenMs;
	private int imageTimeoutMs;
	private boolean verify;

	// 서킷 브레이커 상태
	private final AtomicInteger consecutiveFailures = new AtomicInteger();
	private final AtomicLong openedAt = new AtomicLong();
	private volatile boolean halfOpenProbe;

	@Override
	@PostConstruct
	public void init() throws Exception {
		baseUrl = StringUtils.removeEnd(GlobalsConfig.getPropertyForString("docconverter.rhwp.url", "http://127.0.0.1:8080/rhwp").trim(), "/");
		token = StringUtils.trimToNull(GlobalsConfig.getPropertyForString("docconverter.rhwp.token", ""));
		fallbackName = GlobalsConfig.getPropertyForString("docconverter.rhwp.fallback.class", "HwpLibConverterService");
		fallback = resolveFallback(fallbackName);
		breakerFailures = parseInt(GlobalsConfig.getPropertyForString("docconverter.rhwp.breaker.failures", "3"), 3);
		breakerOpenMs = parseLong(GlobalsConfig.getPropertyForString("docconverter.rhwp.breaker.openms", "300000"), 300000L);
		imageTimeoutMs = parseInt(GlobalsConfig.getPropertyForString("docconverter.rhwp.image.timeoutms", "5000"), 5000);
		verify = !"false".equalsIgnoreCase(GlobalsConfig.getPropertyForString("docconverter.rhwp.verify", "true"));
		logger.info("[RhwpConverterService] url=" + baseUrl + " fallback=" + (fallback == null ? "none" : fallbackName)
				+ " breaker=" + breakerFailures + "/" + breakerOpenMs + "ms verify=" + verify);
		// 기동 시 /health 로 버전·상태를 확인한다. 실패해도 기동은 막지 않는다(브레이커가 처리).
		try {
			JSONObject health = health();
			logger.info("[RhwpConverterService] health=" + health);
		} catch (Exception e) {
			logger.warn("[RhwpConverterService] rhwp.war 에 닿지 못했습니다: " + e.getMessage());
		}
	}

	private DocConverter resolveFallback(String name) {
		if (StringUtils.isBlank(name)) {
			return null;
		}
		if (name.indexOf("HsCellDocConverter") > -1) {
			return hsCellDocConverter;
		} else if (name.indexOf("HwpDocConverter") > -1) {
			return hwpDocDocConverter;
		} else if (name.indexOf("HwpLibConverter") > -1) {
			return hwpLibDocConverter;
		}
		logger.warn("[RhwpConverterService] 알 수 없는 fallback.class=" + name + " — 대체 경로 없음");
		return null;
	}

	// ------------------------------------------------------------------ DocConverter

	@Override
	public void convert(File srcFile, File convertedFile, List<CellAction> actionList) throws Exception {
		if (actionList == null || actionList.isEmpty()) {
			return;
		}
		if (breakerOpen()) {
			fallbackConvert(srcFile, convertedFile, actionList, "서킷 브레이커 열림");
			return;
		}
		List<File> temps = new ArrayList<File>();
		try {
			MultiValueMap<String, Object> form = new LinkedMultiValueMap<String, Object>();
			form.add("file", new FileSystemResource(srcFile));
			form.add("actions", toActionsJson(actionList, form, temps).toString());
			form.add("verify", String.valueOf(verify));
			ResponseEntity<byte[]> resp = post("/convert", form, byte[].class);
			byte[] body = resp.getBody();
			if (resp.getStatusCode() != HttpStatus.OK || body == null || body.length == 0) {
				throw new RhwpException("convert 응답 " + resp.getStatusCode());
			}
			try (FileOutputStream out = new FileOutputStream(convertedFile)) {
				out.write(body);
			}
			String result = decodeResult(resp.getHeaders().getFirst("X-Rhwp-Result"));
			if (logger.isDebugEnabled()) {
				logger.debug("[convert] " + srcFile.getName() + " → " + convertedFile.getName() + " (" + body.length + " bytes) " + result);
			}
			success();
		} catch (UnsupportedActionException e) {
			// 지원하지 않는 액션은 rhwp 장애가 아니다 — 브레이커를 건드리지 않고 대체 구현체로
			fallbackConvert(srcFile, convertedFile, actionList, e.getMessage());
		} catch (Exception e) {
			failure(e);
			fallbackConvert(srcFile, convertedFile, actionList, e.getMessage());
		} finally {
			for (File f : temps) {
				try {
					Files.deleteIfExists(f.toPath());
				} catch (IOException ignored) {
				}
			}
		}
	}

	@Override
	public List<String> getFieldList(File srcFile, List<CellAction> actionList) throws Exception {
		if (breakerOpen()) {
			return fallbackFieldList(srcFile, actionList, "서킷 브레이커 열림");
		}
		try {
			MultiValueMap<String, Object> form = new LinkedMultiValueMap<String, Object>();
			form.add("file", new FileSystemResource(srcFile));
			form.add("option", "2"); // hwplib 구현체와 같이 누름틀·셀 필드 전부
			form.add("number", "0");
			JSONObject json = postJson("/fields", form);
			JSONArray names = json.getJSONArray("names");
			List<String> out = new ArrayList<String>(names.length());
			for (int i = 0; i < names.length(); i++) {
				out.add(names.getString(i));
			}
			success();
			return out;
		} catch (Exception e) {
			failure(e);
			return fallbackFieldList(srcFile, actionList, e.getMessage());
		}
	}

	@Override
	public Map<String, String> getFieldValues(File srcFile, List<CellAction> actionList) throws Exception {
		if (breakerOpen()) {
			return fallbackFieldValues(srcFile, actionList, "서킷 브레이커 열림");
		}
		try {
			JSONArray names = new JSONArray();
			for (CellAction a : actionList) {
				if (a instanceof GetTextValueAction && StringUtils.isNotEmpty(a.getFieldName())) {
					names.put(a.getFieldName());
				}
			}
			Map<String, String> out = new HashMap<String, String>();
			if (names.length() == 0) {
				return out;
			}
			MultiValueMap<String, Object> form = new LinkedMultiValueMap<String, Object>();
			form.add("file", new FileSystemResource(srcFile));
			form.add("fields", names.toString());
			JSONObject json = postJson("/field-values", form);
			JSONObject values = json.getJSONObject("values");
			for (String key : values.keySet()) {
				if (!values.isNull(key)) {
					out.put(key, values.getString(key));
				}
			}
			success();
			return out;
		} catch (Exception e) {
			failure(e);
			return fallbackFieldValues(srcFile, actionList, e.getMessage());
		}
	}

	@Override
	public String getActionMessage(List<CellAction> actionList) throws Exception {
		List<File> temps = new ArrayList<File>();
		try {
			return toActionsJson(actionList, null, temps).toString();
		} finally {
			for (File f : temps) {
				try {
					Files.deleteIfExists(f.toPath());
				} catch (IOException ignored) {
				}
			}
		}
	}

	// ------------------------------------------------------------------ 액션 변환

	/**
	 * BMS {@code CellAction} 목록을 rhwp.war 의 actions JSON 으로. 그림 액션은 URL 을 내려받아
	 * multipart 파트 {@code img<i>} 로 붙이고 {@code imagePart} 로 가리킨다({@code form} 이 null 이면 생략).
	 */
	private JSONArray toActionsJson(List<CellAction> actionList, MultiValueMap<String, Object> form, List<File> temps) throws Exception {
		JSONArray arr = new JSONArray();
		for (int i = 0; i < actionList.size(); i++) {
			CellAction a = actionList.get(i);
			String action = actionName(a);
			JSONObject o = new JSONObject()
					.put("action", action)
					.put("fieldName", a.getFieldName())
					.put("inputValue", a.getInputValue() == null ? "" : a.getInputValue())
					.put("fontName", a.getFontName() == null ? "" : a.getFontName())
					.put("fontType", a.getFontType() == null ? "" : a.getFontType())
					.put("fontSize", a.getFontSize() == null ? "" : a.getFontSize())
					.put("fontColor", a.getFontColor() == null ? "" : a.getFontColor())
					.put("fontBold", a.getFontBold() == null ? "" : a.getFontBold());
			if (a instanceof DefaultAction) {
				o.put("appendLine", ((DefaultAction) a).getAppendLine());
			} else if (a instanceof DefaultSignAction) {
				o.put("appendLine", ((DefaultSignAction) a).getAppendLine());
			}
			if ("insertImage".equals(action) || "appendPicture".equals(action)) {
				String url = a.getInputValue();
				if (StringUtils.isBlank(url)) {
					// hwplib 구현체와 같이 그림이 없으면 아무것도 하지 않는다
					continue;
				}
				if (form != null) {
					File img = downloadImage(url);
					temps.add(img);
					String part = "img" + i;
					form.add(part, new FileSystemResource(img));
					o.put("imagePart", part);
				}
			}
			arr.put(o);
		}
		return arr;
	}

	/** 액션 클래스 → rhwp 액션 이름. hwplib 구현체의 dispatch 와 같은 분류다. */
	private static String actionName(CellAction a) throws UnsupportedActionException {
		if (a instanceof InsertTextExAction || a instanceof InsertSignTextExAction || a instanceof InsertSignDateTextExAction) {
			return "insertTextEx";
		} else if (a instanceof AppendTextExAction || a instanceof AppendSignTextExAction || a instanceof AppendSignDateTextExAction) {
			return "appendTextEx";
		} else if (a instanceof InsertTextAction) {
			return "insertText";
		} else if (a instanceof InsertImageAction) {
			return "insertImage";
		} else if (a instanceof AppendPictureAction) {
			return "appendPicture";
		} else if (a instanceof AddPageCellAction) {
			throw new UnsupportedActionException("addPageCell(을지 병합)은 rhwp 가 지원하지 않습니다");
		}
		throw new UnsupportedActionException("알 수 없는 액션 클래스: " + a.getClass().getName());
	}

	private File downloadImage(String url) throws Exception {
		File img = DocFileUtils.getTempFile();
		HttpURLConnection conn = (HttpURLConnection) new URL(url).openConnection();
		conn.setConnectTimeout(imageTimeoutMs);
		conn.setReadTimeout(imageTimeoutMs);
		conn.setInstanceFollowRedirects(true);
		if (conn.getResponseCode() != 200) {
			throw new RhwpException("서명 이미지 URL 응답 " + conn.getResponseCode() + ": " + url);
		}
		try (InputStream in = conn.getInputStream(); FileOutputStream out = new FileOutputStream(img)) {
			byte[] buf = new byte[16384];
			int n;
			while ((n = in.read(buf)) > 0) {
				out.write(buf, 0, n);
			}
		}
		if (img.length() == 0) {
			throw new RhwpException("서명 이미지가 비어 있습니다: " + url);
		}
		return img;
	}

	// ------------------------------------------------------------------ HTTP

	private <T> ResponseEntity<T> post(String path, MultiValueMap<String, Object> form, Class<T> type) throws Exception {
		HttpHeaders headers = new HttpHeaders();
		headers.setContentType(MediaType.MULTIPART_FORM_DATA);
		if (token != null) {
			headers.set("X-Rhwp-Token", token);
		}
		try {
			return restTemplate.exchange(baseUrl + path, HttpMethod.POST, new HttpEntity<MultiValueMap<String, Object>>(form, headers), type);
		} catch (HttpStatusCodeException e) {
			// 400/401/422/503 — 본문은 {"ok":false,"error":…}
			String body = e.getResponseBodyAsString();
			throw new RhwpException(path + " " + e.getStatusCode().value() + " " + abbreviate(body));
		} catch (ResourceAccessException e) {
			throw new RhwpException(path + " 연결/시간 초과: " + e.getMessage());
		}
	}

	private JSONObject postJson(String path, MultiValueMap<String, Object> form) throws Exception {
		ResponseEntity<String> resp = post(path, form, String.class);
		JSONObject json = new JSONObject(resp.getBody());
		if (!json.optBoolean("ok", false)) {
			throw new RhwpException(path + " 실패: " + json.optString("error"));
		}
		return json;
	}

	public JSONObject health() throws Exception {
		HttpHeaders headers = new HttpHeaders();
		if (token != null) {
			headers.set("X-Rhwp-Token", token);
		}
		ResponseEntity<String> resp = restTemplate.exchange(baseUrl + "/health", HttpMethod.GET, new HttpEntity<Void>(headers), String.class);
		return new JSONObject(resp.getBody());
	}

	private static String decodeResult(String header) {
		if (header == null) {
			return "";
		}
		try {
			return new String(Base64.getDecoder().decode(header), StandardCharsets.UTF_8);
		} catch (IllegalArgumentException e) {
			return header;
		}
	}

	// ------------------------------------------------------------------ 서킷 브레이커·대체 경로

	private boolean breakerOpen() {
		if (breakerFailures <= 0) {
			return false;
		}
		long opened = openedAt.get();
		if (opened == 0) {
			return false;
		}
		if (System.currentTimeMillis() - opened >= breakerOpenMs) {
			// 반개방: 한 건만 시험한다
			if (!halfOpenProbe) {
				halfOpenProbe = true;
				logger.info("[RhwpConverterService] 브레이커 반개방 — 1건 시험");
				return false;
			}
		}
		return true;
	}

	private void success() {
		consecutiveFailures.set(0);
		if (openedAt.get() != 0) {
			logger.info("[RhwpConverterService] 브레이커 닫힘 (rhwp 복구)");
		}
		openedAt.set(0);
		halfOpenProbe = false;
	}

	private void failure(Exception e) {
		int n = consecutiveFailures.incrementAndGet();
		logger.warn("[RhwpConverterService] rhwp 실패 " + n + "회: " + e.getMessage());
		if (halfOpenProbe || (breakerFailures > 0 && n >= breakerFailures)) {
			openedAt.set(System.currentTimeMillis());
			halfOpenProbe = false;
			logger.error("[RhwpConverterService] 브레이커 열림 — " + breakerOpenMs + "ms 동안 " + (fallback == null ? "실패 응답" : fallbackName));
		}
	}

	private void fallbackConvert(File srcFile, File convertedFile, List<CellAction> actionList, String reason) throws Exception {
		if (fallback == null) {
			throw new Exception("[RhwpConverterService] rhwp 변환 실패, 대체 경로 없음: " + reason);
		}
		logger.warn("[RhwpConverterService] 대체 경로 " + fallbackName + " 로 변환: " + reason);
		fallback.convert(srcFile, convertedFile, actionList);
	}

	private List<String> fallbackFieldList(File srcFile, List<CellAction> actionList, String reason) throws Exception {
		if (fallback == null) {
			throw new Exception("[RhwpConverterService] rhwp 필드 조회 실패, 대체 경로 없음: " + reason);
		}
		logger.warn("[RhwpConverterService] 대체 경로 " + fallbackName + " 로 필드 조회: " + reason);
		return fallback.getFieldList(srcFile, actionList);
	}

	private Map<String, String> fallbackFieldValues(File srcFile, List<CellAction> actionList, String reason) throws Exception {
		if (fallback == null) {
			throw new Exception("[RhwpConverterService] rhwp 값 조회 실패, 대체 경로 없음: " + reason);
		}
		logger.warn("[RhwpConverterService] 대체 경로 " + fallbackName + " 로 값 조회: " + reason);
		return fallback.getFieldValues(srcFile, actionList);
	}

	// ------------------------------------------------------------------ util

	private static int parseInt(String s, int def) {
		try {
			return Integer.parseInt(s.trim());
		} catch (Exception e) {
			return def;
		}
	}

	private static long parseLong(String s, long def) {
		try {
			return Long.parseLong(s.trim());
		} catch (Exception e) {
			return def;
		}
	}

	private static String abbreviate(String s) {
		if (s == null) {
			return "";
		}
		return s.length() > 300 ? s.substring(0, 300) + "…" : s;
	}

	/** rhwp.war 호출 실패(연결·상태 코드·처리 실패). 브레이커 실패로 센다. */
	public static class RhwpException extends Exception {
		private static final long serialVersionUID = 1L;

		public RhwpException(String message) {
			super(message);
		}
	}

	/** rhwp 가 다루지 않는 액션. 브레이커 실패로 세지 않고 대체 구현체로 넘긴다. */
	public static class UnsupportedActionException extends Exception {
		private static final long serialVersionUID = 1L;

		public UnsupportedActionException(String message) {
			super(message);
		}
	}
}
